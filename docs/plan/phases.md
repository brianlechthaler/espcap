# Execution phases

Implementing agents should follow this order. One writer per crate. Protocol before firmware and host consumers.

Follow the test skill (TDD + 100% coverage on generated/changed code), the lint skill (`fmt` + `clippy -D warnings`), and the esp32-dev skill (shared `~/.esp32-dev` toolkit) for firmware builds.

Prefer Makefile/CI commands once they exist.

## Gates

```
1. Write failing test(s)          ← TDD Red
2. Implement minimal code         ← TDD Green
3. Refactor if needed
4. Run unit tests                 → must pass
5. Coverage on changed code       → 100%
6. Lint + format check            → must pass
7. Only then: stage / commit / PR
```

- `make test` → `cargo test --workspace --all-targets` excluding the firmware xtensa target
- `make coverage` → llvm-cov on `protocol` + `host` (`--fail-under-functions 100`)
- `make lint` → `cargo fmt --check` + `clippy -D warnings`
- `make firmware` → `with-env.sh cargo build -p espcap-firmware`
- No coverage ignore pragmas unless the user explicitly requests and documents why
- Do not start firmware radio work until protocol tests are green
- Do not mix binary PCAP and JSON on the same TX stream

CI (later): lint + test + coverage on Ubuntu for host/protocol. Firmware job only if a reusable esp-idf action is reliable; otherwise document local `make firmware`.

## Phase 0 — Record plans (this PR)

Write `docs/plan/*.md` and a short README. No firmware or CLI code.

## Phase 1 — Workspace + protocol

Cargo workspace, Makefile, protocol crate: command serde, filter engine, radiotap/PCAP builders, JSON event types. Tests red then green. Lint + 100% coverage on protocol.

## Phase 2 — Host CLI skeleton

Depends on protocol. clap, serialport TX of commands, RX JSON ack/status, manufacturer → OUI. Tests with mock serial. No firmware required.

## Phase 3 — Firmware skeleton

Depends on protocol. `esp-idf-svc` hello, dual-core spawn, UART loopback of `get` / `set` / `status`. Flash via `~/.esp32-dev`.

## Phase 4 — WiFi path

NULL/STA promiscuous FFI, hop task, discovery parse of beacon/probe, capture enqueue. Golden 802.11 fixtures in protocol tests.

## Phase 5 — BLE path

`esp32-nimble` (or IDF NimBLE FFI) passive scan, enqueue ads, BLE PCAP reconstruct tests.

## Phase 6 — Encoder + CLI capture

JSON TX and framed PCAP TX; CLI `start --json` / `start --pcap`; drop counters. End-to-end with mocked UART if no hardware.

## Phase 7 — User docs + CI

`docs/getting-started.md`, `docs/architecture.md`, `docs/features/*`, GitHub Actions. README stays short and links into `docs/`.

## Parallelism

Safe in the same phase: independent files with a single owner. Not safe: two agents editing `crates/protocol` or the same firmware module.

Parent agent owns integration, Makefile, and workspace `Cargo.toml`.
