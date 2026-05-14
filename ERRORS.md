# ERRORS.md

Check this before suggesting approaches to tasks similar to logged ones.

---

## macOS C++ stdlib headers not found during Rust build (librocksdb-sys)

**What didn't work:**
1. `SDKROOT=$(xcrun --show-sdk-path)` exported in shell before `cargo build` — not sufficient; librocksdb-sys build script uses `env -u` which can strip it.
2. `CPLUS_INCLUDE_PATH = "/Library/Developer/CommandLineTools/usr/include/c++/v1"` in `.cargo/config.toml` — path exists as a directory but is empty on this system; `cstdint` is not there.

**What worked:** Set both variables in `.cargo/config.toml` pointing at the SDK path:
```toml
[env]
CPLUS_INCLUDE_PATH = "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/c++/v1"
CXXFLAGS = "-isystem /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/c++/v1"
```

**Note for next time:** Always verify the header file actually exists (`ls <path>/cstdint`) before using the path. The CLT installs headers inside the SDK, not at the bare `/usr/include/c++/v1` path.

---

## devimint dev-fed hangs silently (missing esplora/lnd)

**What didn't work:**
1. `devimint dev-fed -- bash -c "..."` — wrong syntax, `--` is not supported.
2. `devimint dev-fed --exec bash -c "..."` — correct syntax, but `dev-fed` requires esplora + lnd + ldk-gateway binaries that aren't built. Hangs after spawning bitcoind with no error output.

**What worked:** Bypass devimint entirely. Start bitcoind manually in regtest, start 4 fedimintd processes directly via env vars, run DKG via `fedimint-cli admin setup` commands, then run the spammer.

**Note for next time:** `devimint dev-fed` is for full Lightning integration testing. For federation-only benchmarks, start the stack manually.

---

## Spammer HTTP transport (wrong API protocol)

**What didn't work:** Using `reqwest` to POST to `{ws_url}/transaction` — fedimint's API is WebSocket JSON-RPC, not HTTP REST.

**What worked:** Use `DynGlobalApi::new(connectors, peers, None)` from `fedimint-api-client` with a `ConnectorRegistry` from `ConnectorRegistry::build_from_client_defaults().bind().await`. Then call `api.submit_transaction(tx).await`.

**Note for next time:** All fedimint transaction submission goes through `IGlobalFederationApi::submit_transaction` over WebSocket. No HTTP REST endpoint exists for this.

---

## bench.sh killing fedimintd with jobs -p xargs kill

**What didn't work:** `jobs -p | xargs kill` after DKG completion — kills ALL background jobs including the 4 fedimintd processes, not just the start-dkg jobs. AlephBFT crashed 1.6s after consensus start. Spammer connected but got no response (server dying).

**What worked:** Track fedimintd PIDs explicitly in `FEDIMINTD_PIDS=()` array when spawning them, then selectively kill only non-fedimintd background jobs after DKG.

**Note for next time:** Never use `jobs -p | xargs kill` when you need some background jobs to keep running. Always track PIDs explicitly.

---

## Docker image build fails with edition2024 on Rust 1.80

**What didn't work:** `FROM rust:1.80-slim-bookworm` — Rust 1.80 doesn't support `edition = "2024"` in Cargo.toml (stabilized in 1.85).

**What worked:** `FROM rust:1.85-slim-bookworm` initially; upgraded to `rust:1.88-slim-bookworm` when `time@0.3.47` required MSRV 1.88.0.

**Note for next time:** edition2024 requires Rust ≥ 1.85. Check `cargo tree -e normal | grep ^time` for MSRV conflicts before picking a Rust base image. As of this workspace, minimum is 1.88.

---

## duration_constructors unstable in Docker build (Rust ≤ 1.88)

**What didn't work:** `Duration::from_mins()` and `Duration::from_hours()` are behind the `duration_constructors` feature gate — not stabilized in Rust 1.88. Fedimint upstream uses them in 35+ places across `fedimint-logging`, `fedimint-core`, `fedimint-server`, and several modules.

**What worked:** Global replacement across all `.rs` files:
- `from_mins(N)` → `from_secs(N * 60)`
- `from_hours(N)` → `from_secs(N * 3600)`

**Note for next time:** If the Docker Rust version is ever bumped past stabilization (likely 1.89+), these can be reverted to the cleaner `from_mins`/`from_hours` form.

---

## Docker build fails: cmake can't find make / rustfmt missing

**What didn't work:** `rust:1.88-slim-bookworm` + `debian:bookworm-slim` don't include `make` or `rustfmt`. CMake defaults to "Unix Makefiles" generator and panics when `make` is absent. `rustfmt` absence causes a non-fatal warning that can be silenced.

**What worked:** Add `make` to the apt-get install step; add `rustup component add rustfmt` in the same RUN layer.

**Note for next time:** Any slim Rust Docker image needs `make` added explicitly alongside `cmake`. Check both are present before building.

---

## macOS .cargo/config.toml env vars break Docker Linux build

**What didn't work:** Building inside Docker while `.cargo/config.toml` has macOS-specific `CPLUS_INCLUDE_PATH` and `CXXFLAGS` pointing at `/Library/Developer/CommandLineTools/...` — those paths don't exist in Linux containers.

**What worked:** In the Dockerfile, set `ENV CPLUS_INCLUDE_PATH="" CXXFLAGS="" CFLAGS=""` before the cargo build step. Cargo's `[env]` section only sets vars that aren't already in the environment (force=false default), so the Docker ENV takes precedence.

**Note for next time:** Always override macOS-specific env vars in Dockerfiles for this workspace.

---

## jemalloc configure failure (path with spaces)

**What didn't work:** Default Cargo target directory inherits workspace path; path contained "Claude Code" with a space, which jemalloc's `./configure` rejects.

**What worked:** Set `target-dir = "/tmp/hellas-falcon-build"` in `fedimint-src/.cargo/config.toml`.

**Note for next time:** Any project under a path with spaces will hit this. Always set a space-free `target-dir` for jemalloc-dependent workspaces.
