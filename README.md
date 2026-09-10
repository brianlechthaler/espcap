# espcap

ESP32-S3 and ESP32-C5 firmware plus a host CLI for WiFi promiscuous capture and BLE advertisement scanning.

Promiscuous capture of other people's traffic may be restricted. Use this only on networks and devices you are authorized to monitor.

## Docs

- [Getting started](docs/getting-started.md)
- [Architecture](docs/architecture.md)
- [CLI](docs/features/cli.md)
- [WiFi](docs/features/wifi.md)
- [BLE](docs/features/ble.md)
- [Implementation plan](docs/plan/index.md)

## Build

```bash
make test
make lint
make firmware-s3
make firmware-c5
make flash-s3
```

Requires the shared ESP32 toolkit at `~/.esp32-dev`. Host commands use `/dev/ttyACM*` (USB Serial/JTAG). `make firmware-c5` compiles only; do not flash C5 until that board is attached.
