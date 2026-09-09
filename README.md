# espcap

ESP32 firmware and host CLI for WiFi promiscuous capture and BLE advertisement scanning, focused on device and AP discovery.

This repository currently holds the implementation plan only. Firmware and CLI land in later PRs.

## Documentation

- [Plan index](docs/plan/index.md)
- [Architecture](docs/plan/architecture.md)
- [On-wire protocol](docs/plan/protocol.md)
- [Capture behavior](docs/plan/capture.md)
- [Firmware](docs/plan/firmware.md)
- [Host CLI](docs/plan/host.md)
- [Execution phases](docs/plan/phases.md)

## Requirements (planned)

- Original ESP32 (WROOM-32 / DevKitC), dual-core
- Shared ESP32 toolkit at `~/.esp32-dev` (ESP-IDF + esp rustc)
- Host: Rust stable, USB serial (`/dev/ttyUSB*` or `/dev/ttyACM*`)

## License

TBD
