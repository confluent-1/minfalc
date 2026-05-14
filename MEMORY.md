# MEMORY.md

Read this at the start of every session before doing anything.

---

## 2026-05-05, Signature Architecture

**What was decided:** Embed Falcon public keys directly in the `FalconSigWithKey` struct inside `TransactionSignature::FalconMultisig`, rather than modifying `InputMeta` or `ClientInput`.
**Why:** Keeps server-side code unchanged. No need to modify the 10+ `ClientInput` construction sites. Falcon variant is self-contained and verifiable without external key registry.
**What was rejected:** Adding `falcon_keys` field to `ClientInput` — would require changes across too much of the existing codebase.

---

## 2026-05-06, Federation Startup — Bypass devimint

**What was decided:** Bypass `devimint dev-fed` entirely. bench.sh manually starts bitcoind (regtest), 4× fedimintd, and runs DKG via `fedimint-cli admin setup` commands.
**Why:** `devimint dev-fed` requires esplora + lnd + ldk-gateway binaries. Without them it hangs silently after spawning bitcoind. We don't need Lightning for this benchmark.
**What was rejected:** `devimint dev-fed` — too heavy, requires Lightning stack we don't have or need.

---

## 2026-05-05, Build Target Directory

**What was decided:** Set `target-dir = "/tmp/hellas-falcon-build"` in `fedimint-src/.cargo/config.toml`.
**Why:** jemalloc's `configure` script rejects paths containing spaces; the default target dir inherits the workspace path which contains "Claude Code".
**What was rejected:** Renaming the project folder — user's folder, not ours to rename.

---

## 2026-05-05, macOS C++ Headers Fix

**What was decided:** Set `CXXFLAGS` and `CPLUS_INCLUDE_PATH` to `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/c++/v1` in `.cargo/config.toml`.
**Why:** `librocksdb-sys` build script uses `env -u IPHONEOS_DEPLOYMENT_TARGET` which strips some env. The headers are NOT at `/Library/Developer/CommandLineTools/usr/include/c++/v1/` (that path is empty) — they are inside the SDK at the path above.
**What was rejected:** `/Library/Developer/CommandLineTools/usr/include/c++/v1` — directory exists but is empty on this system. `SDKROOT` alone was not sufficient.

---

## 2026-05-12, Docker Deployment

**What was decided:** Run the 4 fedimintd guardians in Docker containers (2 vCPU + 2 GB each) with the spammer and fedimint-cli running natively on the host.
**Why:** Single-machine bare-metal results were capped at ~330 accepted TPS due to 4 processes fighting for the same CPU. Docker gives CPU isolation and a more realistic inter-container network (~0.5–2ms latency). Absolute TPS still won't match geo-distributed deployment but comparison metrics (block size, latency delta) will be meaningful.
**What was rejected:** Skipping Docker — single-machine numbers aren't defensible to the Hellas team.

---

## 2026-05-12, Falcon Module Instance ID

**What was decided:** Falcon module is instance ID 0. Modules are assigned IDs alphabetically by kind string during DKG. "falcon-transfer" sorts before "ln", "lnv2", "meta", "mint", "wallet".
**Why:** Confirmed via guardian log: "Initialise module 0..." followed by lnv2=2, mint=4, wallet=5.
**What was rejected:** N/A — this was a discovery, not a choice.

---

## Session Summary, 2026-05-14 (second session)

**Worked on:** Diagnosing multi-level TPS sweep anomaly; planning bench.sh federation-per-level restart fix.

**Completed:** Nothing new — this session was interrupted immediately after context was restored. The task (modify bench.sh to restart federation between TPS levels) was not executed.

**In progress:** Modifying bench.sh to wrap federation startup + DKG + teardown inside the TPS for-loop so each level gets a fresh Docker volume.

**Decisions made:** None new this session.

**Next session — START HERE:**
Modify `bench.sh` to restart the federation between TPS levels. The fix is:
1. Extract federation startup + DKG block (currently lines ~108–199) into a reusable block inside the TPS loop.
2. Add `docker compose -f "$COMPOSE_FILE" down -v` at the end of each TPS iteration (after writing the CSV row).
3. Move BTC_CLI / API_PORTS / FM_AUTH_PASS constants outside the loop (static), but keep the `docker compose up`, bitcoind wait, guardian wait, DKG, and consensus-wait inside.
4. The CSV header write stays outside the loop (once).
5. Cleanup trap stays for EXIT (abnormal exits only).

Expected structure:
```bash
# outside loop: constants, CSV header
for TPS in "${TPS_LEVELS[@]}"; do
  docker compose up -d --no-build
  # wait for bitcoind, mine 101 blocks
  # wait for guardians
  # DKG
  # wait for consensus
  run falcon-spammer
  parse output, write CSV row
  docker compose down -v
done
# summary generation (outside loop)
```

After the fix: run `./bench.sh --scheme falcon512` and `./bench.sh --scheme ed25519`, then commit and push to https://github.com/confluent-1/minfalc.

---

## 2026-05-12, Benchmark UTXO Check Removed

**What was decided:** Removed UTXO balance check from `process_input` in `fedimint-falcon-server/src/lib.rs`.
**Why:** Every transaction was rejected with `NotEnoughFunds` because no UTXOs were pre-funded. The benchmark measures Falcon-512 signature throughput, not fund accounting. Signature verification still happens in full.
**What was rejected:** Pre-funding UTXOs — unnecessary complexity for a throughput benchmark.

---

## Session Summary, 2026-05-14

**Worked on:** Phase A and Phase B tx size reduction.

**Completed:**
- Phase A: removed `public_key` from `FalconSigWithKey`; pubkey now lives in `FalconInput.falcon_pub_key` (passed via `InputMeta.falcon_pub_key: Option<Vec<u8>>`); `FalconOutput` uses `falcon_pk_hash: [u8; 32]`. Expected ~1,660B/tx.
- Phase B: `FalconOutput` is now an enum (`Transfer { recipient_account_id: u32 }` / `Register { pub_key_bytes: Vec<u8> }`). `FalconInput.account_id: u32`. Server stores key registry `FalconAccountKey(u32) → Vec<u8>`. Spammer does a registration phase before the benchmark loop. **Actual measured: 730B/tx** (pre-run estimate was ~750B).
- All crates check clean (zero errors, zero warnings): `fedimint-core`, `fedimint-falcon-common`, `fedimint-falcon-server`, `fedimint-falcon-client`, `falcon-spammer`, `fedimint-server`, `fedimint-server-ui`.

**Key design decisions:**
- `account_id = u32::from_le_bytes(sha256(pubkey)[0..4])` — deterministic, computed client-side, no need to poll output_status
- Registration tx: 0 inputs, 1 `Register` output, `NaiveMultisig([])` — passes funding and sig checks because all amounts are zero
- Ed25519 baseline also registers via `Register` outputs (storing 33-byte secp key); `process_input` returns `falcon_pub_key: Some(secp_bytes)` but NaiveMultisig path never uses it

**Benchmark results (measured):**
- Falcon-512: **730 B/tx**
- Ed25519: **134 B/tx**
- Single-level fresh runs confirm 500 TPS works (~99% accepted) for both schemes
- Multi-level sweep runs show false cliff at 500 TPS — this is stale federation state from prior levels, NOT the real ceiling
- bench_summary.md ceiling of ~228-229 TPS is an artifact of the broken sweep and is meaningless
- Real ceiling is between 500 TPS (confirmed working) and 1000 TPS (confirmed 0 accepted on sweep); exact value unknown until per-level restart runs complete

**Clean benchmark results (1000–1400 TPS range, fresh federation per level):**

| Target TPS | F-512 rej% | F-512 latency | Ed25519 rej% | Ed25519 latency |
|---|---|---|---|---|
| 1000 | 0.0% | 0ms | 1.7% | 0ms |
| 1100 | 0.0% | 0ms | 2.3% | 0ms |
| 1200 | 0.0% | 3ms | 1.9% | 0ms |
| 1300 | 0.0% | 190ms | 0.7% | 0ms |
| 1400 | ~0.0% | 1,233ms | 2.9% | 0ms |

**Key findings:**
- Falcon-512 ceiling: ~1300 TPS (0 rejections, latency climbing but acceptable; 1400 TPS latency explodes to 1.2s)
- Ed25519 ceiling: ~1300 TPS (consistent 1-3% rejections throughout range, zero latency buildup)
- Bottleneck is BFT consensus rounds, not signature size or verification
- Falcon-512 costs 5.4x in tx size (730B vs 134B), essentially nothing in throughput

**Summary output:** `bench_summary.html` (HTML, opens in browser — replaced bench_summary.md)

**Next session:**
- Commit and push everything to https://github.com/confluent-1/minfalc

---

## Session Summary, 2026-05-13

**Worked on:** Full Docker build pipeline, first complete benchmark run, transaction size diagnosis, Ed25519 baseline spammer groundwork.

**Completed:**
- Fixed Docker build: rust:1.85→1.88, added `make`, rustfmt, replaced 35+ `Duration::from_mins/from_hours` with stable `from_secs`
- Fixed `lncm/bitcoind:v27.0.0` → `v27.0`, fixed double `bitcoind` command in docker-compose.yml
- First full benchmark run complete — bytes/tx = 3,422, ceiling ~325 accepted TPS
- Added CSV output + `--scheme ed25519|falcon512` to spammer and bench.sh
- Added `bench_summary.md` auto-generation (comparison table) after each run
- Diagnosed 3,422-byte tx: 897-byte Falcon pubkey included **3× per tx** — that's the bug

**Byte breakdown:**
- FalconInput.falcon_pub_key: 897B
- FalconOutput.falcon_pub_key: 897B
- FalconSigWithKey.public_key: 897B (duplicate)
- FalconSigWithKey.signature: ~666B
- Overhead: ~65B
- Falcon-512 hard floor: 1,563B (pubkey + sig) — no compression possible

**Next session — two-phase tx size reduction (START HERE):**

**Phase A — Fix triple-key bug → ~1,660 bytes**
1. Remove `public_key` from `FalconSigWithKey` — verifier reads key from input
2. Replace `FalconOutput.falcon_pub_key` (897B) with 32-byte `sha256(falcon_pk)` as address
3. Files: `fedimint-falcon-common/src/lib.rs`, `fedimint-falcon-client/src/lib.rs`, `fedimint-falcon-server/src/lib.rs`, `bench/spammer/src/main.rs`

**Phase B — Key registration → ~750 bytes per transfer**
1. Add `FalconRegister` tx type: user submits 897B pubkey once, guardian stores it, returns 4-byte `account_id`
2. `FalconInput` carries `account_id` (4B) instead of `falcon_pub_key` (897B)
3. `FalconOutput` carries `recipient_account_id` (4B) instead of key material
4. Server looks up pubkey by `account_id` to verify
5. Registration tx: ~1,600B one-time. Transfer tx: ~750B ✓

**After size work:** run Ed25519 baseline (`./bench.sh --scheme ed25519`), then commit and push to https://github.com/confluent-1/minfalc

---

## Session Summary, 2026-05-06

**Worked on:** Getting the full benchmark pipeline running end-to-end — fixing the build environment, federation startup, and spammer transport.

**Completed:**
- All 4 binaries build: `fedimintd`, `fedimint-cli`, `falcon-spammer`, `falcon-dashboard`
- Installed missing system deps: `cmake`, `autoconf`, `protobuf`, `bitcoin` (all via brew)
- Fixed macOS C++ header path — `cstdint` is at SDK path, not bare CLT path
- Replaced `devimint dev-fed` with manual federation startup in bench.sh (bitcoind + 4× fedimintd + DKG via fedimint-cli)
- Rewrote spammer to use `DynGlobalApi` WebSocket transport instead of broken HTTP POST
- Created `CLAUDE.md`, `MEMORY.md`, `ERRORS.md` in project root
- Added session-start rule to CLAUDE.md requiring Read of MEMORY.md and ERRORS.md

**In progress:** `./bench.sh` was running at session end — had just started and was waiting for guardians to enter DKG setup mode. Unknown whether it completed successfully.

**Decisions made:**
- devimint bypassed in favour of manual startup (see decision entry above)
- Spammer uses `DynGlobalApi` over WS (see ERRORS.md)

**Next session:**
1. Read MEMORY.md and ERRORS.md first (required by CLAUDE.md).
2. Check whether the bench run completed. If it errored, paste the output and debug from there.
3. Likely next issues to watch for: DKG setup status polling (ConsensusIsRunning check), spammer connecting to the federation API after DKG, module instance ID mismatch (falcon module may not be instance 0).
4. Once a clean run completes, commit everything and push to https://github.com/confluent-1/minfalc.
