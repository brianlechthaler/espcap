# espcap

ESP32-S3 and ESP32-C5 firmware and host CLI for WiFi promiscuous capture and BLE advertisement scanning, focused on device and AP discovery.

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

- ESP32-S3 (`xtensa-esp32s3-espidf`) and ESP32-C5 (`riscv32imac-esp-espidf`)
- Shared ESP32 toolkit at `~/.esp32-dev` (ESP-IDF + esp rustc)
- Host: Rust stable, USB serial (`/dev/ttyACM*`; `/dev/ttyUSB*` only if a board exposes a USB-UART bridge)

## License

TBD
