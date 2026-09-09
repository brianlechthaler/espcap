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

- `make test` → `cargo test --workspace --all-targets` excluding firmware targets (`xtensa-esp32s3-espidf`, `riscv32imac-esp-espidf`)
- `make coverage` → llvm-cov on `protocol` + `host` (`--fail-under-functions 100`)
- `make lint` → `cargo fmt --check` + `clippy -D warnings`
- `make firmware-s3` → `MCU=esp32s3 with-env.sh cargo build -p espcap-firmware --target xtensa-esp32s3-espidf`
- `make firmware-c5` → `MCU=esp32c5 with-env.sh cargo build -p espcap-firmware --target riscv32imac-esp-espidf`
- No `make firmware` that implies original ESP32 / WROOM
- No coverage ignore pragmas unless the user explicitly requests and documents why
- Do not start firmware radio work until protocol tests are green
- Do not mix binary PCAP and JSON on the same TX stream
- Flash and serial tests run on S3 and C5 hardware only. Do not add WROOM jobs or “untested original ESP32” paths

CI (later): lint + test + coverage on Ubuntu for host/protocol. Firmware job only if a reusable esp-idf action can build **both** S3 and C5; otherwise document local `make firmware-s3` and `make firmware-c5`.

## Phase 0 — Record plans (this PR)

Write `docs/plan/*.md` and a short README. No firmware or CLI code.

## Phase 1 — Workspace + protocol

Cargo workspace, Makefile, protocol crate: command serde (including `wifi_band` / `channels_5ghz` / `freq_mhz`), filter engine, radiotap/PCAP builders for 2.4 and 5 GHz, JSON event types. Tests red then green. Lint + 100% coverage on protocol.

## Phase 2 — Host CLI skeleton

Depends on protocol. clap, serialport TX of commands, RX JSON ack/status, manufacturer → OUI, S3 vs C5 band validation from `status.chip`. Tests with mock serial. No firmware required.

## Phase 3 — Firmware skeleton

Depends on protocol. `esp-idf-svc` hello on **both** targets: USB serial loopback of `get` / `set` / `status` with `chip` / `wifi_bands`. S3 may spawn pinned Core1 threads; C5 must not. Flash via `~/.esp32-dev`.

## Phase 4 — WiFi path

NULL/STA promiscuous FFI, hop task (S3 2.4 GHz; C5 2.4 + 5 GHz), discovery parse of beacon/probe, capture enqueue. Golden 802.11 fixtures in protocol tests for both bands.

## Phase 5 — BLE path

`esp32-nimble` (or IDF NimBLE FFI) passive scan including extended ads, enqueue ads, BLE PCAP reconstruct tests. Same app code on S3 and C5.

## Phase 6 — Encoder + CLI capture

JSON TX and framed PCAP TX; CLI `start --json` / `start --pcap`; drop counters. End-to-end with mocked serial if no hardware. Hardware bring-up on S3 and C5 when available.

## Phase 7 — User docs + CI

`docs/getting-started.md`, `docs/architecture.md`, `docs/features/*`, GitHub Actions. README stays short and links into `docs/`. Getting started must cover both chips and `/dev/ttyACM*`.

## Parallelism

Safe in the same phase: independent files with a single owner. Not safe: two agents editing `crates/protocol` or the same firmware module.

Parent agent owns integration, Makefile, and workspace `Cargo.toml`.
