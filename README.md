# hellas-falcon-bench

A 4-node Fedimint BFT federation benchmarking harness that replaces client-side
transaction signatures with **Falcon-512** (post-quantum) from the
`pqcrypto-falcon` crate. Guardian consensus signatures (threshold BLS/FROST)
are untouched.

---

## What was changed

| Location | Change |
|---|---|
| `fedimint-core/src/transaction.rs` | Added `FalconMultisig(Vec<FalconSigWithKey>)` variant to `TransactionSignature`; `validate_signatures` now handles Falcon path—public keys are embedded in the sig pairs, no parameter-signature changes |
| `fedimint-core/Cargo.toml` | Added `pqcrypto-falcon`, `pqcrypto-traits` |
| `modules/fedimint-falcon-common` | New module: shared `FalconInput` / `FalconOutput` types, `FalconModuleTypes` |
| `modules/fedimint-falcon-server` | New module: server-side UTXO ledger, `process_input` / `process_output` |
| `modules/fedimint-falcon-client` | New module: `sign_transaction_falcon()`, secp placeholder helper |
| `bench/spammer` | `falcon-spammer` binary — `--tps`, `--duration` |
| `bench/dashboard` | `falcon-dashboard` binary — ratatui TUI + CSV metrics |
| `docker/hellas-falcon/` | `Dockerfile.guardian`, `Dockerfile.client`, `docker-compose.yml` |

**Guardian consensus signatures are not touched.** They remain threshold BLS
(via `crypto/tbs/`) and are entirely separate from the transaction signing path.

---

## Prerequisites

- **Rust 1.87+** with Cargo (`rustup update stable`)
- **Docker** 24+ and **Docker Compose** v2 (`docker compose version`)
- **clang / libclang** (required by `pqcrypto-falcon`'s C bindings):
  ```
  # macOS
  brew install llvm
  # Ubuntu/Debian
  apt-get install clang libclang-dev
  ```

---

## Build (local, without Docker)

```bash
cd fedimint-src

# Full workspace check
cargo check --workspace

# Build all bench binaries
cargo build --release --bin falcon-spammer --bin falcon-dashboard

# Artifacts:
#   target/release/falcon-spammer
#   target/release/falcon-dashboard
```

---

## Run with Docker Compose

### 1. Build images

```bash
cd fedimint-src/docker/hellas-falcon
docker compose build
```

This compiles the full workspace inside the builder container — takes 5–15 min
on first run, cached on rebuilds.

### 2. Start the federation

```bash
docker compose up -d
```

Four guardian containers (`hellas-guardian-0..3`) and one client container
(`hellas-client`) come up on the `falcon-net` bridge (172.30.0.0/24).

Check that all guardians are healthy:

```bash
docker compose ps
# All four guardians should show "healthy"
```

### 3. Complete DKG

On first start, guardians need to complete distributed key generation. Open the
federation setup UI on any guardian:

```
http://localhost:8175
```

Follow the wizard to complete DKG across all four nodes. After DKG the
federation is live and the Falcon-transfer module is registered.

### 4. Run the transaction spammer

```bash
# Default: 50 TPS for 60 seconds
docker compose exec client falcon-spammer \
  --federation-url ws://guardian-0:8174 \
  --tps 50 \
  --duration 60

# Benchmark curve: 10 → 500 TPS
for TPS in 10 50 100 250 500; do
  docker compose exec client falcon-spammer \
    --federation-url ws://guardian-0:8174 \
    --tps $TPS \
    --duration 30
done
```

### 5. Watch the live dashboard

```bash
docker compose exec -it client falcon-dashboard \
  --federation-url http://guardian-0:8174 \
  --csv-path /data/metrics.csv
```

Press `q` or `Esc` to exit. The dashboard shows:
- Block height
- Last block size (bytes)
- Falcon sig bytes per block
- Finality latency (ms)
- Cumulative tx count
- Live TPS gauge (0–1000 scale)

### 6. Retrieve CSV metrics

```bash
# Copy to host after the run
docker compose cp client:/data/metrics.csv ./metrics.csv
```

CSV columns:
```
timestamp_ms, height, block_size_bytes, falcon_sig_bytes, finality_latency_ms, cumulative_tx_count
```

### 7. Stop and clean up

```bash
docker compose down -v   # -v removes named volumes (resets all guardian state)
```

---

## Understanding the output numbers

### Falcon-512 vs ed25519 signature size comparison

| Scheme | Public key | Signature | Total per input |
|---|---|---|---|
| ed25519 | 32 B | 64 B | 96 B |
| secp256k1 Schnorr | 33 B | 64 B | 97 B |
| **Falcon-512** | **897 B** | **~690 B avg** | **~1,587 B** |

Falcon-512 signatures are **~16× larger** than Schnorr per input. A block
containing 100 Falcon-signed transactions carries roughly **159 KB of signature
data** vs ~9.7 KB for Schnorr. The `falcon_sig_bytes` column in the CSV lets
you measure this directly.

### Block size numbers

`block_size_bytes` = total encoded bytes of transactions in that consensus
block. This includes inputs, outputs, nonces, and `FalconMultisig` payloads.
The fraction attributable to Falcon is `falcon_sig_bytes / block_size_bytes`.

At 100 TPS with a ~500 ms block interval:

| Column | Expected value |
|---|---|
| block_size_bytes | ~79 KB (50 txs × ~1.6 KB each) |
| falcon_sig_bytes | ~79 KB (signature is ~99% of tx size) |
| finality_latency_ms | 200–800 ms (AlephBFT 4-node regtest) |

### Why block sizes grow with Falcon

Falcon-512 public keys (897 B) are embedded in every transaction's
`FalconMultisig` field. This is deliberate: it makes verification self-contained
and avoids a separate key-registry lookup. The tradeoff is ~10× larger
transactions compared to Schnorr. For deployments where key registration is
acceptable, the signature alone (690 B avg) would suffice, reducing overhead to
~8× larger.

---

## Non-goals (explicit)

- No real economic security — the UTXO balances have no external backing
- No persistent state across `docker compose down -v` runs
- No p2p discovery — all URLs are static Docker service hostnames
- No mainnet-compatible serialization — `FalconMultisig` uses fedimint's
  internal `Encodable` derive with no versioning guarantees

---

## Architecture

```
┌─────────────────────────── falcon-net (172.30.0.0/24) ──────────────────────┐
│                                                                               │
│  guardian-0:8173/8174  ──┐                                                   │
│  guardian-1:8173/8174  ──┤  AlephBFT consensus  (BLS threshold signatures)  │
│  guardian-2:8173/8174  ──┤  fedimintd + falcon-transfer module               │
│  guardian-3:8173/8174  ──┘                                                   │
│                                                                               │
│  client  ──── falcon-spammer ──→ ws://guardian-0:8174 (submit txs)          │
│         └──── falcon-dashboard → http://guardian-0:8174 (poll status)        │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

Transaction signing path:
```
spammer → build_bench_tx() → sign_transaction_falcon()
                              └─ pqcrypto_falcon::falcon512::detached_sign(txid, sk)
                              └─ Transaction { signatures: FalconMultisig([{pk, sig}]) }
                              └─ submit to federation API

guardian → process_transaction_with_dbtx()
         └─ process_input()  (FalconModule: checks UTXO balance)
         └─ validate_signatures()
              └─ FalconMultisig branch:
                   for each (pk, sig): falcon512::verify_detached_signature(sig, txid, pk)
```
