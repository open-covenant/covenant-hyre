# Architecture

## End-to-end sequence

```
+---------+      MCP tool call         +-----------+
| agent   | -------------------------> | covenantd |
+---------+                            +-----+-----+
                                             |
                       capability + budget    |
                       gating, payer = caller |
                                             v
                                  +----------+----------+
                                  | covenant-hyre       |
                                  | (402-then-pay loop) |
                                  +----------+----------+
                                             |
              GET /defi/tvl                  |
              <------------------------------+
              | 402 + {accepts:[{
              |   pay_to,
              |   maxAmountRequired,
              |   extra: {feePayer: <PayAI sponsor>}
              | }]}
              v
        +-----+------+
        |   Hyre     |
        +-----+------+
              |
              |  (covenant-hyre pins pay_to + feePayer
              |   against config constants, rejects
              |   substitution attacks)
              |
              |  forks to sidecar via stdin (PaymentRequirements JSON)
              v
        +-----+----------------------+
        | covenant-x402-signer       |
        | (separate process,         |
        |  holds funder keypair)     |
        +-----+----------------------+
              |
              |  build v0 VersionedTransaction:
              |    payerKey = PayAI sponsor
              |    instructions:
              |      1. ComputeBudget.set_compute_unit_limit(20000)
              |      2. ComputeBudget.set_compute_unit_price(1)
              |      3. spl_token.transfer_checked(funder_ata, mint, dst_ata,
              |                                     funder, amount, decimals)
              |  partial-sign as funder only; fee_payer slot stays empty
              |  serialize -> base64 -> wrap in x402 envelope -> base64 header
              v
        stdout: <X-PAYMENT header value>
              |
              v
        +-----+------+
        |   Hyre     |  GET /defi/tvl + X-PAYMENT
        +-----+------+
              |
              |  middleware POSTs to PayAI:
              v
        +-----+-------------------+
        | facilitator.payai.network |
        |   POST /verify            |
        |   POST /settle            |
        +-----+-------------------+
              |
              |  PayAI fills the fee_payer signature slot,
              |  broadcasts the tx
              v
        +-----+------+
        |   Solana   |  USDC settled, slot recorded
        +-----+------+
              |
              v
        Hyre returns 200 + JSON to covenantd
              |
              v
        covenantd writes:
          - SettlementReceipt (resource = Tool, credits_consumed)
          - AuditEvent (ExternalPaymentSettled, issuer = operator, payer = caller)
          - Budget debited
        Returns the response to the agent.
```

## Why partial-sign

PayAI sponsors the SOL gas, so the v0 message's `payerKey` is *their* pubkey, not ours. If the funder signed everything, we'd be paying gas in SOL on top of the USDC. By leaving signature slot 0 (the fee-payer slot) empty and only signing slot 1 (the funder), PayAI's `/settle` co-signs as fee payer and broadcasts. The funder spends zero SOL.

## Why instruction order matters

PayAI's facilitator validates the layout. ComputeBudget instructions must come first (`set_compute_unit_limit(20000)` then `set_compute_unit_price(1)`), then exactly one `TransferChecked`. Deviation gets rejected at `/verify`. The order is documented in `@payai/x402`'s `transaction-builder.ts`. We mirror it in [`covenant-x402/src/payai.rs::build_payai_transaction`](https://github.com/open-covenant/covenant/blob/hyre-integration/agent-os/crates/covenant-x402/src/payai.rs).

## Why a sidecar process

Two reasons:

1. **Funder key isolation.** The daemon's address space never holds the keypair. A bug in covenantd cannot exfiltrate the key.
2. **Dependency-tree isolation.** `solana-sdk` v4 conflicts with the Anchor 0.31 + `five8` stack the rest of covenant transitively pulls in. The sidecar lives in its own cargo workspace, so the funder-signing crate stays buildable independently. The covenantd binary stays slim.

## Wire-format pins

Three checks happen before signing, rejecting a manipulated 402 challenge:

| Pin | Constant | What it stops |
|---|---|---|
| `pay_to` | `covenant_hyre::config::PAY_TO` = `7G73…` | A MITM'd or compromised Hyre swapping in an attacker address |
| `extra.feePayer` | `covenant_hyre::config::PAYAI_FEE_PAYER` = `2wKup…` | A substituted sponsor pubkey ending up in the v0 `payerKey` slot |
| `asset` (mint) | `covenant_hyre::config::USDC_MINT` = `EPjFW…` | A non-USDC mint slipping into `transfer_checked` |

In addition, the sidecar runs `getAccountInfo` against the funder's USDC ATA before building the transaction. PayAI removed on-the-fly ATA creation, so a missing source ATA would otherwise fail silently at `/settle` *after* the X-PAYMENT envelope is consumed.

## Network spelling

Hyre and PayAI both accept either `"solana"` (short) or `"solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"` (CAIP-2). PayAI's v1 wire expects the short form in the envelope, so we normalize CAIP-2 to `"solana"` before serializing the payload. Hyre's 402 body uses `"solana"` short; the CAIP-2 form is the operator's capability identifier internally.

## Settlement attribution

Two distinct AgentIds flow through `DaemonHyreExecutor::execute`:

- **payer**, the agent that invoked the MCP tool. Their budget is debited; the settlement receipt is recorded against them.
- **issuer**, the operator identity. Recorded as the audit event's issuer.

This split lets a multi-tenant Covenant host run agents that pay for their own Hyre calls without giving the agent direct access to the funding key.
