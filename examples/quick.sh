#!/usr/bin/env bash
# Smallest-possible reproducer of the Covenant <-> Hyre x402 paid call.
# Requires: bash, curl, jq, the covenant-x402-signer sidecar binary built from
# https://github.com/open-covenant/covenant (agent-os/crates/covenant-x402-signer).
#
# Funder requirements: a Solana keypair with USDC and an EXISTING USDC ATA on
# the cluster this RPC points at. PayAI does not create ATAs on the hot path.
# The funder pays only USDC; PayAI pays SOL gas as the sponsored fee payer.
#
# Spends ~$0.01 USDC against the live Hyre /defi/tvl endpoint when executed.

set -euo pipefail

: "${COVENANT_X402_SIGNER_BIN:?set to the built covenant-x402-signer binary}"
: "${COVENANT_X402_FUNDING_KEYPAIR:?set to the funder Solana keypair JSON path}"
COVENANT_X402_RPC_URL="${COVENANT_X402_RPC_URL:-https://api.mainnet-beta.solana.com}"

ENDPOINT="${HYRE_ENDPOINT:-https://mpp.hyreagent.fun/defi/tvl}"
PER_CALL_CAP_ATOMIC=50000     # 0.05 USDC ceiling
PINNED_PAY_TO="7G73PLhKvAPBGTzG5ESAE4coE7QrVeTTKfhTxQZbyGgC"
PINNED_FEE_PAYER="2wKupLR9q6wXYppw8Gr2NvWxKBUqm4PPJKkQfoxHDBg4"
PINNED_USDC_MINT="EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"

for cmd in curl jq "$COVENANT_X402_SIGNER_BIN"; do
  command -v "$cmd" >/dev/null 2>&1 || { echo "missing: $cmd" >&2; exit 1; }
done

echo "[1/4] fetching the 402 challenge (free)..."
challenge=$(curl -sS -o - -w "\n__STATUS__%{http_code}" "$ENDPOINT")
status=${challenge##*__STATUS__}
body=${challenge%__STATUS__*}
body=${body%$'\n'}
if [ "$status" != "402" ]; then
  echo "expected 402 from $ENDPOINT, got $status" >&2
  echo "$body" | head -c 400 >&2; echo >&2
  exit 1
fi

echo "[2/4] picking option + verifying security pins..."
option=$(jq -c --argjson cap "$PER_CALL_CAP_ATOMIC" '
  .accepts
  | map(select(.scheme == "exact"))
  | map(select((.maxAmountRequired // .amount | tonumber) <= $cap))
  | first
' <<<"$body")
if [ -z "$option" ] || [ "$option" = "null" ]; then
  echo "no acceptable option within cap $PER_CALL_CAP_ATOMIC" >&2
  exit 2
fi
pay_to=$(jq -r '.payTo'                       <<<"$option")
fee_payer=$(jq -r '.extra.feePayer // empty'  <<<"$option")
asset=$(jq -r '.asset'                        <<<"$option")
amount=$(jq -r '.maxAmountRequired // .amount' <<<"$option")
[ "$pay_to"    = "$PINNED_PAY_TO"     ] || { echo "pay_to pin failed: $pay_to"          >&2; exit 3; }
[ "$fee_payer" = "$PINNED_FEE_PAYER"  ] || { echo "feePayer pin failed: $fee_payer"     >&2; exit 3; }
[ "$asset"     = "$PINNED_USDC_MINT"  ] || { echo "asset pin failed: $asset"            >&2; exit 3; }
echo "    pinned pay_to + fee_payer + asset OK, amount $amount atomic USDC"

echo "[3/4] piping PaymentRequirements to the sidecar..."
requirements=$(jq -n \
  --arg amount "$amount" \
  --arg asset "$asset" \
  --arg pay_to "$pay_to" \
  --arg fee_payer "$fee_payer" \
  '{
    network: "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp",
    asset: $asset,
    amount: $amount,
    amountUsdc: (($amount | tonumber) / 1000000),
    payTo: $pay_to,
    scheme: "exact",
    extra: { feePayer: $fee_payer }
  }')
header=$(echo "$requirements" | \
  COVENANT_X402_FUNDING_KEYPAIR="$COVENANT_X402_FUNDING_KEYPAIR" \
  COVENANT_X402_RPC_URL="$COVENANT_X402_RPC_URL" \
  "$COVENANT_X402_SIGNER_BIN")
[ -n "$header" ] || { echo "sidecar returned empty header" >&2; exit 4; }
echo "    signed, X-PAYMENT length=${#header}"

echo "[4/4] re-POSTing with X-PAYMENT..."
final=$(curl -sS -o - -w "\n__STATUS__%{http_code}" \
  -H "x-payment: $header" "$ENDPOINT")
final_status=${final##*__STATUS__}
final_body=${final%__STATUS__*}
final_body=${final_body%$'\n'}
echo "    status=$final_status"
echo "$final_body" | head -c 800; echo
if [ "$final_status" != "200" ]; then
  echo "FAIL: expected 200, got $final_status" >&2
  exit 5
fi
echo "OK: $amount atomic USDC paid; tx is on chain. Look up your funder's recent transactions to grab the signature."
