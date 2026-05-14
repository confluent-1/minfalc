#!/usr/bin/env bash
# hellas-falcon-bench  —  one-command benchmark runner
#
# Usage:
#   ./bench.sh              # runs at 500, 1000, 2500, 5000, 10000 TPS
#   ./bench.sh --tps 1000   # runs at a single TPS level
#   ./bench.sh --build-only # builds host tools + Docker image, doesn't run
#
# Hardware label: 4-guardian Falcon-512 BFT · Docker · Apple M4 · 2 vCPU + 2 GB / guardian
# Network:        Docker bridge, ~0.5–2 ms inter-container latency

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$SCRIPT_DIR/fedimint-src"
BINS="/tmp/hellas-falcon-build/release"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
BENCH_LABEL="4-guardian Falcon-512 BFT · Docker · Apple M4 · 2 vCPU + 2 GB / guardian"

TPS_LEVELS=(1000 1100 1200 1300 1400)
DURATION=30   # seconds per TPS level
SINGLE_TPS=""
BUILD_ONLY=false
SCHEME="falcon512"

# ── arg parsing ───────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case $1 in
    --tps)       SINGLE_TPS="$2"; shift 2 ;;
    --duration)  DURATION="$2";   shift 2 ;;
    --scheme)    SCHEME="$2";     shift 2 ;;
    --build-only) BUILD_ONLY=true; shift ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

CSV_FILE="$SCRIPT_DIR/bench_results_${SCHEME}_$(date +%Y%m%d_%H%M%S).csv"

if [[ -n "$SINGLE_TPS" ]]; then
  TPS_LEVELS=("$SINGLE_TPS")
fi

# ── prerequisites ─────────────────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  hellas-falcon-bench  ·  Falcon-512 BFT benchmark"
echo "  $BENCH_LABEL"
echo "═══════════════════════════════════════════════════════════════"
echo ""

for cmd in cargo bitcoin-cli docker jq; do
  if ! command -v "$cmd" &>/dev/null; then
    echo "ERROR: '$cmd' not found."
    exit 1
  fi
done

if ! docker info &>/dev/null; then
  echo "ERROR: Docker is not running. Start Docker Desktop and try again."
  exit 1
fi

# ── build host tools (fedimint-cli, falcon-spammer) ──────────────────────────
echo "▶ Building host tools (fedimint-cli, falcon-spammer)..."
cd "$SRC"

export SDKROOT
SDKROOT="$(xcrun --show-sdk-path)"

cargo build --release \
  --bin fedimint-cli \
  --bin falcon-spammer \
  2>&1 | grep -E "^error|Compiling fedimint-cli|Compiling falcon-spammer|Finished" || true

for b in fedimint-cli falcon-spammer; do
  if [[ ! -x "$BINS/$b" ]]; then
    echo "ERROR: '$b' did not build."
    exit 1
  fi
done

export PATH="$BINS:$PATH"
echo "✔ Host tools ready"
echo ""

# ── build Docker image ────────────────────────────────────────────────────────
echo "▶ Building Docker image (first run: ~10 min; cached after that)..."
docker compose -f "$COMPOSE_FILE" build guardian-0
echo "✔ Docker image ready"
echo ""

if [[ "$BUILD_ONLY" == "true" ]]; then
  echo "Build-only mode. Run ./bench.sh to execute the benchmark."
  exit 0
fi

# ── cleanup on exit ───────────────────────────────────────────────────────────
cleanup() {
  echo ""
  echo "Shutting down..."
  jobs -p | xargs kill 2>/dev/null || true
  docker compose -f "$COMPOSE_FILE" down -v 2>/dev/null || true
}
trap cleanup EXIT

# ── static config ─────────────────────────────────────────────────────────────
BTC_RPC_PORT=18443
BTC_RPC_USER="bitcoin"
BTC_RPC_PASS="bitcoin"
BTC_CLI="bitcoin-cli -regtest -rpcport=$BTC_RPC_PORT -rpcuser=$BTC_RPC_USER -rpcpassword=$BTC_RPC_PASS"
API_PORTS=(8174 8184 8194 8204)
FM_AUTH_PASS="pass"
FM_FEDERATION_API="ws://127.0.0.1:${API_PORTS[0]}"

echo "  Hardware: $BENCH_LABEL"
echo ""

# ── CSV header ────────────────────────────────────────────────────────────────
printf "hardware,target_tps,achieved_tps,duration_s,submitted,accepted,rejected,avg_latency_ms,bytes_per_tx\n" \
  > "$CSV_FILE"

for TPS in "${TPS_LEVELS[@]}"; do
  # ── start fresh federation for this TPS level ─────────────────────────────
  echo "▶ Starting federation (TPS=$TPS)..."
  docker compose -f "$COMPOSE_FILE" up -d --no-build

  until $BTC_CLI getblockchaininfo &>/dev/null; do sleep 1; done
  $BTC_CLI createwallet "bench" &>/dev/null || $BTC_CLI loadwallet "bench" &>/dev/null || true
  ADDR=$($BTC_CLI getnewaddress)
  $BTC_CLI generatetoaddress 101 "$ADDR" &>/dev/null
  echo "  ✔ bitcoind ready (101 blocks mined)"

  for i in 0 1 2 3; do
    until fedimint-cli --password "$FM_AUTH_PASS" \
      admin setup "ws://127.0.0.1:${API_PORTS[$i]}" status &>/dev/null 2>&1; do
      sleep 1
    done
  done
  echo "  ✔ All guardians ready for DKG"

  # ── DKG ───────────────────────────────────────────────────────────────────
  PEER_INFOS=()
  for i in 0 1 2 3; do
    if [[ $i -eq 0 ]]; then
      info=$(fedimint-cli --password "$FM_AUTH_PASS" \
        admin setup "ws://127.0.0.1:${API_PORTS[$i]}" \
        set-local-params "Guardian $i" \
        --federation-name "hellas-falcon-bench" \
        --federation-size 4 | jq -r .)
    else
      info=$(fedimint-cli --password "$FM_AUTH_PASS" \
        admin setup "ws://127.0.0.1:${API_PORTS[$i]}" \
        set-local-params "Guardian $i" | jq -r .)
    fi
    PEER_INFOS+=("$info")
  done

  for i in 0 1 2 3; do
    for j in 0 1 2 3; do
      if [[ $i -ne $j ]]; then
        fedimint-cli --password "$FM_AUTH_PASS" \
          admin setup "ws://127.0.0.1:${API_PORTS[$i]}" \
          add-peer "${PEER_INFOS[$j]}" &>/dev/null
      fi
    done
  done

  for i in 0 1 2 3; do
    fedimint-cli --password "$FM_AUTH_PASS" \
      admin setup "ws://127.0.0.1:${API_PORTS[$i]}" \
      start-dkg &>/dev/null &
  done

  echo -n "  Waiting for consensus to start (up to 600s)..."
  READY=false
  for _ in $(seq 1 300); do
    if docker compose -f "$COMPOSE_FILE" logs guardian-0 2>/dev/null \
        | grep -q "Starting consensus session"; then
      READY=true
      break
    fi
    printf "."
    sleep 2
  done
  echo ""

  if [[ "$READY" != "true" ]]; then
    echo "ERROR: consensus did not start within 600s at TPS=$TPS"
    echo "       Check logs: docker compose logs guardian-0"
    exit 1
  fi

  jobs -p | xargs kill 2>/dev/null || true
  echo "  ✔ DKG complete — consensus is running"
  echo ""

  # ── run spammer ───────────────────────────────────────────────────────────
  echo "─── TPS=$TPS duration=${DURATION}s ───"
  output=$(falcon-spammer \
    --federation-url "$FM_FEDERATION_API" \
    --tps "$TPS" \
    --duration "$DURATION" \
    --scheme "$SCHEME" 2>&1)
  echo "$output"
  echo ""

  achieved=$(echo "$output" | grep "Achieved TPS:"  | awk '{print $NF}')
  submitted=$(echo "$output" | grep "Submitted:"    | awk '{print $NF}')
  accepted=$(echo "$output"  | grep "Accepted:"     | awk '{print $NF}')
  rejected=$(echo "$output"  | grep "Rejected:"     | awk '{print $NF}')
  latency=$(echo "$output"   | grep "Avg latency:"  | awk '{print $NF}' | tr -d 'ms')
  bytes_tx=$(echo "$output"  | grep "Bytes/tx:"     | awk '{print $NF}')

  printf "%s,%s,%s,%s,%s,%s,%s,%s,%s\n" \
    "$BENCH_LABEL" "$TPS" "$achieved" "$DURATION" \
    "$submitted" "$accepted" "$rejected" "$latency" "$bytes_tx" \
    >> "$CSV_FILE"

  # ── tear down federation before next level ────────────────────────────────
  docker compose -f "$COMPOSE_FILE" down -v
  echo "✔ Federation torn down (TPS=$TPS)"
  echo ""
done

echo "✔ Benchmark complete."
echo "  Hardware: $BENCH_LABEL"
echo "  Results:  $CSV_FILE"

# ── comparison summary (HTML) ─────────────────────────────────────────────────
SUMMARY="$SCRIPT_DIR/bench_summary.html"
FALCON_CSV=$(ls -t "$SCRIPT_DIR"/bench_results_falcon512_*.csv 2>/dev/null | head -1)
ED25519_CSV=$(ls -t "$SCRIPT_DIR"/bench_results_ed25519_*.csv 2>/dev/null | head -1)

F_BYTES=$(awk -F, 'NR==2 {print $9}' "$FALCON_CSV"  2>/dev/null || echo "—")
E_BYTES=$(awk -F, 'NR==2 {print $9}' "$ED25519_CSV" 2>/dev/null || echo "—")
RATIO="—"
if [[ "$F_BYTES" =~ ^[0-9]+$ && "$E_BYTES" =~ ^[0-9]+$ ]]; then
  RATIO=$(awk "BEGIN {printf \"%.1fx\", $F_BYTES / $E_BYTES}")
fi

{
cat <<HTML
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>hellas-falcon-bench</title>
<style>
  body { font-family: -apple-system, sans-serif; max-width: 900px; margin: 40px auto; padding: 0 20px; color: #1a1a1a; }
  h1 { font-size: 1.4rem; margin-bottom: 4px; }
  .meta { color: #666; font-size: 0.85rem; margin-bottom: 32px; }
  h2 { font-size: 1rem; text-transform: uppercase; letter-spacing: 0.05em; color: #555; margin: 32px 0 12px; }
  table { border-collapse: collapse; width: 100%; font-size: 0.9rem; }
  th { background: #f4f4f4; text-align: left; padding: 8px 12px; border-bottom: 2px solid #ddd; white-space: nowrap; }
  td { padding: 7px 12px; border-bottom: 1px solid #eee; }
  tr:last-child td { border-bottom: none; }
  .good { color: #1a7f37; font-weight: 600; }
  .warn { color: #b35000; }
  .num { text-align: right; }
</style>
</head>
<body>
<h1>hellas-falcon-bench — Comparison Summary</h1>
<div class="meta">${BENCH_LABEL}<br>Generated: $(date)</div>

<h2>Transaction Size</h2>
<table>
  <tr><th>Scheme</th><th class="num">Bytes / tx</th><th>vs Ed25519</th></tr>
  <tr><td>Ed25519</td><td class="num">${E_BYTES}</td><td>baseline</td></tr>
  <tr><td>Falcon-512</td><td class="num">${F_BYTES}</td><td>${RATIO}</td></tr>
</table>
HTML

if [[ -f "$FALCON_CSV" && -f "$ED25519_CSV" ]]; then
cat <<HTML
<h2>Per-Level Results</h2>
<table>
  <tr>
    <th>Target TPS</th>
    <th class="num">F-512 accepted</th><th class="num">F-512 rej%</th><th class="num">F-512 latency</th>
    <th class="num">Ed25519 accepted</th><th class="num">Ed25519 rej%</th><th class="num">Ed25519 latency</th>
  </tr>
HTML
  awk -F, '
    FNR==NR && NR>1 {
      tps=$2; f_acc[tps]=$6; f_sub[tps]=$5; f_rej[tps]=$7; f_lat[tps]=$8
      n++; order[n]=tps; next
    }
    FNR>1 { tps=$2; e_acc[tps]=$6; e_sub[tps]=$5; e_rej[tps]=$7; e_lat[tps]=$8 }
    END {
      for (i=1; i<=n; i++) {
        t = order[i]
        f_pct = (f_sub[t]>0) ? sprintf("%.1f%%", f_rej[t]/f_sub[t]*100) : "—"
        f_cls = (f_rej[t]==0) ? "good" : "warn"
        if (t in e_sub) {
          e_pct = (e_sub[t]>0) ? sprintf("%.1f%%", e_rej[t]/e_sub[t]*100) : "—"
          e_cls = (e_rej[t]==0) ? "good" : "warn"
          printf "  <tr><td>%s</td><td class=\"num\">%s</td><td class=\"num %s\">%s</td><td class=\"num\">%sms</td><td class=\"num\">%s</td><td class=\"num %s\">%s</td><td class=\"num\">%sms</td></tr>\n", \
            t, f_acc[t], f_cls, f_pct, f_lat[t], e_acc[t], e_cls, e_pct, e_lat[t]
        } else {
          printf "  <tr><td>%s</td><td class=\"num\">%s</td><td class=\"num %s\">%s</td><td class=\"num\">%sms</td><td>—</td><td>—</td><td>—</td></tr>\n", \
            t, f_acc[t], f_cls, f_pct, f_lat[t]
        }
      }
    }
  ' "$FALCON_CSV" "$ED25519_CSV"
  echo "</table>"
elif [[ -f "$FALCON_CSV" ]]; then
cat <<HTML
<h2>Falcon-512 Results</h2>
<table>
  <tr><th>Target TPS</th><th class="num">Accepted</th><th class="num">Rej%</th><th class="num">Latency</th></tr>
HTML
  awk -F, 'NR>1 {
    pct = ($5>0) ? sprintf("%.1f%%", $7/$5*100) : "—"
    cls = ($7==0) ? "good" : "warn"
    printf "  <tr><td>%s</td><td class=\"num\">%s</td><td class=\"num %s\">%s</td><td class=\"num\">%sms</td></tr>\n", $2, $6, cls, pct, $8
  }' "$FALCON_CSV"
  echo "</table>"
elif [[ -f "$ED25519_CSV" ]]; then
cat <<HTML
<h2>Ed25519 Results</h2>
<table>
  <tr><th>Target TPS</th><th class="num">Accepted</th><th class="num">Rej%</th><th class="num">Latency</th></tr>
HTML
  awk -F, 'NR>1 {
    pct = ($5>0) ? sprintf("%.1f%%", $7/$5*100) : "—"
    cls = ($7==0) ? "good" : "warn"
    printf "  <tr><td>%s</td><td class=\"num\">%s</td><td class=\"num %s\">%s</td><td class=\"num\">%sms</td></tr>\n", $2, $6, cls, pct, $8
  }' "$ED25519_CSV"
  echo "</table>"
fi

echo "</body></html>"

} > "$SUMMARY"

echo "  Summary:  $SUMMARY"
