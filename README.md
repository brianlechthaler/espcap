# espcap

ESP32-S3 and ESP32-C5 firmware plus a host CLI for WiFi promiscuous capture and BLE advertisement scanning.

Promiscuous capture of other people's traffic may be restricted. Use this only on networks and devices you are authorized to monitor.

## Docs

- [Getting started](docs/getting-started.md)
- [Architecture](docs/architecture.md)
- [Documentation index](docs/index.md)
- [CLI](docs/features/cli.md)
- [WiFi](docs/features/wifi.md)
- [BLE](docs/features/ble.md)
- [Serial protocol](docs/features/protocol.md)
- [Implementation plan](docs/plan/index.md)

## Build

```bash
make test
make lint
make firmware-s3
make firmware-c5
make flash-s3
make flash-c5
```

Requires the shared ESP32 toolkit at `~/.esp32-dev`. `make flash-s3` and `make flash-c5` probe USB serial ports and flash the matching chip. `S3_PORT` and `C5_PORT` override that. `make devices` prints each port and chip. Host commands still take `--port` (see [Getting started](docs/getting-started.md)).
