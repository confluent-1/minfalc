# Planned Updates

Two independent work items. Do padded-sig switch first — the commonware sim should use the correct signature format from the start.

---

## 1. Switch to Padded FN-DSA Signatures

**Why:** The current implementation uses `pqcrypto_falcon::falcon512::detached_sign()`, which outputs the compressed variable-length format (~666B average, up to ~809B max). FIPS 206 (FN-DSA) standardizes a padded fixed-length variant where every signature is the same size. Fixed size makes block accounting deterministic and aligns with the standard.

**Files to change:**
- `fedimint-core/src/transaction.rs` — verification: swap `falcon512::DetachedSignature::from_bytes` for padded equivalent
- `modules/fedimint-falcon-client/src/lib.rs` — signing: swap `falcon512::detached_sign` for padded variant
- `bench/spammer/src/main.rs` — any direct signing calls

**First step:** confirm whether `pqcrypto-falcon` exposes a `falcon512padded` module. If yes, it's a straightforward swap. If not, evaluate `pqcrypto-falcon-sys` directly or a different crate that implements the FIPS 206 padded format.

**Expected outcome:** `FalconSigWithKey.signature` becomes a fixed-size `[u8; N]` (or `Vec<u8>` of constant length), and the `FALCON512_TRANSFER_BYTES_APPROX` constant in the client becomes exact.

---

## 2. Commonware-Runtime Network Simulation

**Why:** The existing Docker benchmark measures raw TPS under near-zero latency (~0.5–2ms inter-container). The Hellas team is building on `commonware-runtime` and needs to know how Falcon-512 verification cost interacts with consensus latency under realistic geo-distributed network conditions. This is a separate simulation crate — it does not refactor Fedimint's P2P layer.

**Approach:** New crate at `bench/sim/` using `commonware-runtime`'s deterministic module. Models 4-guardian BFT consensus rounds in-process with injected latency/jitter/loss. Reuses Falcon-512 signing and verification from `fedimint-falcon-client` and `fedimint-core`. Deterministic execution via fixed seed — same seed reproduces identical results.

**Parameters to sweep:**
- `latency_ms`: baseline one-way inter-guardian delay (e.g. 10, 50, 100, 200ms)
- `jitter_ms`: per-message latency variance
- `loss_pct`: packet loss rate (0, 1, 5%)
- `tps_target`: transaction submission rate

**Output:** CSV matching existing bench format — `scheme, latency_ms, jitter_ms, loss_pct, tps_target, tps_accepted, finality_ms` — so results can be dropped into `bench_summary.html` alongside the Docker numbers.

**Coordination note:** Hellas team to confirm preferred integration pattern before merging — they may want this folded into their own commonware-based stack rather than kept as a standalone bench crate.

**Does not touch:** Fedimint's existing P2P networking, AlephBFT internals, Docker benchmark, or any production code paths.
