# Proof of payment

The integration was verified end-to-end on Solana mainnet on 2026-05-28. Both the inner 402-then-pay loop (`covenant_hyre::execute_paid`) and the daemon path (`covenantd::hyre::DaemonHyreExecutor`) produce wire-compatible output that lands on chain.

**Re-verified 2026-06-17.** After merging the provider profile to `open-covenant/covenant` `main` and bounding the paid HTTP path with a request timeout, the same envelope was re-checked against `facilitator.payai.network/verify` (free, no settle): `isValid: true`, payer `DkhgMfHAMdiCd9jAz1Ahok9d9EjV8kESepbDMeKFU5i3`. No wire-format drift — the transaction below remains the canonical on-chain proof.

## The transaction

- **Signature:** [`CpXZJtu2M7jevuYZUMAEzC2tSFut2fevmTB6DG7DEyu9iGVt7x23F1wmLv1P5JCjTiajVPrcYf44uX8ByP7x1nP`](https://solscan.io/tx/CpXZJtu2M7jevuYZUMAEzC2tSFut2fevmTB6DG7DEyu9iGVt7x23F1wmLv1P5JCjTiajVPrcYf44uX8ByP7x1nP)
- **Slot:** 422733766
- **Block time:** 2026-05-28T15:57:49+02:00
- **Cluster:** mainnet-beta
- **Status:** Ok
- **Fee:** 0.000010001 SOL (paid by PayAI, not the funder)

## What was paid for

```
GET https://mpp.hyreagent.fun/defi/tvl
```

Response: 200, JSON body with TVL rankings across DeFi chains (Ethereum $41.9B, BSC $5.4B, Solana $5.2B, …). Endpoint price: $0.01 USDC = 10000 atomic units.

## Decoded transaction structure

### Signatures section

```
sig count (compact-u16): 0x02
sig[0]: <PayAI's signature, 64 bytes>   <-- co-signed by PayAI at /settle
sig[1]: <funder's signature, 64 bytes>  <-- written by covenant-x402-signer
                                            during partial-sign
```

If you decode our raw output *before* it hits PayAI, sig[0] is all zeros. PayAI fills it during `/settle`.

### Message (v0)

```
version byte: 0x80                       <-- v0 indicator
header:
  num_required_signatures:     2
  num_readonly_signed:         1         <-- funder is sr-- (signed, read-only)
  num_readonly_unsigned:       3         <-- token program, mint, ComputeBudget
account keys (7 total):
  [0] 2wKupLR9…DBg4   srw-   PayAI fee payer
  [1] DkhgMfH…U5i3   sr--   funder (covenant-agent.json equivalent in the live test)
  [2] 42RoMNf…spVz   -rw-   funder's USDC ATA
  [3] FaZxn8A…pXBs   -rw-   recipient's USDC ATA
  [4] Compute…11111   -r-x   ComputeBudget program
  [5] Tokenke…5DA    -r-x   SPL Token program
  [6] EPjFWdd…Dt1v   -r--   USDC mint
recent_blockhash: <fresh from getLatestBlockhash("confirmed")>
```

### Instructions (in PayAI-required order)

| # | Program | Data | Meaning |
|---|---|---|---|
| 0 | ComputeBudget | `[2, 32, 78, 0, 0]` | `set_compute_unit_limit(20000)`, discriminator 2, then u32 LE = 20000 |
| 1 | ComputeBudget | `[3, 1, 0, 0, 0, 0, 0, 0, 0]` | `set_compute_unit_price(1)` µlamports, discriminator 3, then u64 LE = 1 |
| 2 | SPL Token | `[12, 16, 39, 0, 0, 0, 0, 0, 0, 6]` | `TransferChecked`, discriminator 12, amount u64 LE = 10000, decimals = 6 |

Instruction 2's accounts: `[funder_ata, mint, recipient_ata, funder, funder]`. The last two `funder` references are the authority and the signer-pubkey list; PayAI never appears in this instruction.

## Balance deltas

| Account | Before | After | Δ |
|---|---|---|---|
| PayAI fee payer (`2wKup…`) | 0.221584646 SOL | 0.221574645 SOL | −0.000010001 SOL (gas) |
| Funder USDC ATA (`42RoM…`) | 0.002039 SOL (rent), 1.998 USDC | 0.002039 SOL, 1.988 USDC | −0.01 USDC |
| Recipient USDC ATA (`FaZxn…`) | 0.080001 USDC | 0.090001 USDC | +0.01 USDC |

No SOL change on the funder.

## Daemon-side accounting (in-memory ledgers, separate live run)

The same flow through `DaemonHyreExecutor` also wrote:

- 1 `SettlementReceipt`, id `45d98ff9-c993-4951-9207-da8afa493aa6`, resource `Tool`, `credits_consumed = 10000`, payer = the calling agent.
- 1 `AuditEvent`, kind `ExternalPaymentSettled { provider: "hyre", endpoint: "https://mpp.hyreagent.fun/defi/tvl", amount: "10000", … }`, issuer = the operator.
- Budget debit: 100000 → 90000 credits on the payer.

## How to reproduce

See [`examples/quick.sh`](../examples/quick.sh) for the bash version or [`examples/rust-binary/`](../examples/rust-binary/) for the typed Rust version. Both spawn the same `covenant-x402-signer` sidecar internally; both reproduce the exact wire format above.
