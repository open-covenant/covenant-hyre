# Rust reproducer

Same flow as [`../quick.sh`](../quick.sh), typed. Spawns the
`covenant-x402-signer` sidecar as a subprocess; no covenant crates as
dependencies. Useful as a starting point for embedding the integration into
an unrelated Rust service.

```sh
export COVENANT_X402_SIGNER_BIN=/abs/path/to/covenant-x402-signer
export COVENANT_X402_FUNDING_KEYPAIR=~/.config/solana/funder.json
export COVENANT_X402_RPC_URL=https://api.mainnet-beta.solana.com   # optional

cargo run --release
```

Spends ~$0.01 USDC against `mpp.hyreagent.fun/defi/tvl`. Funder must hold USDC
and have an existing USDC ATA on the cluster the RPC points at.
