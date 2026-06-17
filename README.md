# covenant-hyre

Open Covenant agents calling [Hyre](https://hyreagent.fun) as paid HTTP, settled in USDC on Solana through [PayAI's facilitator](https://docs.payai.network/x402/facilitators/introduction).

[![CI](https://github.com/open-covenant/covenant-hyre/actions/workflows/ci.yml/badge.svg)](https://github.com/open-covenant/covenant-hyre/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## What it is

A reference integration showing how a Covenant daemon agent buys a Hyre API call without ever holding the funder key in-process. The funder partial-signs a v0 `VersionedTransaction`, Hyre's middleware forwards it to PayAI, PayAI co-signs as fee payer and lands the transfer, and Hyre returns the data. Standard x402, sponsored-gas variant.

This repo is a small, self-contained showcase: two runnable examples (bash + Rust), the on-chain proof of a real call, and the wire-format details a third party would need to reproduce.

**Status:** live. The provider profile is merged to [`open-covenant/covenant`](https://github.com/open-covenant/covenant) `main` ([`hyre-x402/v1`](https://github.com/open-covenant/covenant/releases/tag/hyre-x402/v1)), the paid HTTP path is bounded with a request timeout, and the envelope was re-verified against PayAI's facilitator on 2026-06-17 (`/verify` → `isValid: true`). See [`docs/proof.md`](docs/proof.md).

## How it works

```
agent -> covenantd -> covenant-hyre -> Hyre /defi/tvl
                                          | 402 + accepts[extra.feePayer]
                            covenant-x402-signer (sidecar)
                                          | funder-signed v0 VersionedTransaction
                                          | base64 in X-PAYMENT header
                            Hyre /defi/tvl (retry)
                                          | middleware POSTs to:
                            PayAI /verify, /settle
                                          | co-sign + broadcast
                            Solana mainnet
                                          | USDC transfer lands
                            agent <- 200 + JSON data
```

Full sequence in [`docs/architecture.md`](docs/architecture.md).

## Try it

Smallest taste, bash + the upstream sidecar binary:

```sh
# 1. Build the funder-key sidecar (it has its own workspace).
git clone https://github.com/open-covenant/covenant && cd covenant
cd agent-os/crates/covenant-x402-signer && cargo build --release

# 2. Configure: a Solana keypair with USDC and an existing USDC ATA.
export COVENANT_X402_SIGNER_BIN=$PWD/target/release/covenant-x402-signer
export COVENANT_X402_FUNDING_KEYPAIR=~/.config/solana/your-funder.json
export COVENANT_X402_RPC_URL=https://api.mainnet-beta.solana.com

# 3. Run the bash example.
cd /path/to/covenant-hyre
bash examples/quick.sh
# expects: 200 + TVL JSON, $0.01 USDC moved
```

A typed Rust version of the same flow lives in [`examples/rust-binary/`](examples/rust-binary/).

## Proof

| | |
|---|---|
| On-chain sig | [`CpXZJtu2M7jevuYZUMAEzC2tSFut2fevmTB6DG7DEyu9iGVt7x23F1wmLv1P5JCjTiajVPrcYf44uX8ByP7x1nP`](https://solscan.io/tx/CpXZJtu2M7jevuYZUMAEzC2tSFut2fevmTB6DG7DEyu9iGVt7x23F1wmLv1P5JCjTiajVPrcYf44uX8ByP7x1nP) |
| Slot | 422733766 |
| Cluster | mainnet-beta |
| Endpoint paid | `GET https://mpp.hyreagent.fun/defi/tvl` |
| Amount | 10000 atomic USDC ($0.01) |
| Funder (slot 1, signed) | `DkhgMfHAMdiCd9jAz1Ahok9d9EjV8kESepbDMeKFU5i3` |
| Fee payer (slot 0, PayAI sponsor) | `2wKupLR9q6wXYppw8Gr2NvWxKBUqm4PPJKkQfoxHDBg4` |
| Recipient | `7G73PLhKvAPBGTzG5ESAE4coE7QrVeTTKfhTxQZbyGgC` |

Full decoded byte structure (v0 message, instruction order, signature placement) in [`docs/proof.md`](docs/proof.md).

## Repo layout

```
covenant-hyre/
  README.md
  LICENSE                      # Apache-2.0
  docs/
    architecture.md            # sequence, byte layout, design notes
    proof.md                   # the live tx, fully decoded
  examples/
    quick.sh                   # bash + curl + sidecar binary
    rust-binary/               # standalone Rust binary, same flow
  .github/workflows/ci.yml     # shellcheck + cargo build/clippy
```

## Upstreams

- **Covenant**, [`open-covenant/covenant`](https://github.com/open-covenant/covenant). The production integration lives at `agent-os/crates/covenant-hyre` and the funding-key sidecar at `agent-os/crates/covenant-x402-signer`. Tracking PR: [#71](https://github.com/open-covenant/covenant/pull/71).
- **Hyre**, [`mpp.hyreagent.fun`](https://mpp.hyreagent.fun). Live manifest at [`/openapi.json`](https://mpp.hyreagent.fun/openapi.json).
- **PayAI x402 facilitator**, [`facilitator.payai.network`](https://facilitator.payai.network), client lib [`PayAINetwork/x402-solana`](https://github.com/PayAINetwork/x402-solana).

## Maintainers

- Open Covenant: [@open-covenant](https://github.com/open-covenant)
- Hyre: [@cryptoeights](https://github.com/cryptoeights)

## License

[Apache-2.0](LICENSE).
