# Getting started

Promiscuous capture of other people's traffic may be restricted. Use this only on networks and devices you are authorized to monitor.

## Hardware

| Board | USB serial | WiFi |
|-------|------------|------|
| ESP32-S3 | `/dev/ttyACM0` (USB Serial/JTAG) | 2.4 GHz |
| ESP32-C5 | `/dev/ttyACM1` (USB Serial/JTAG) | 2.4 GHz and 5 GHz |

`make flash-s3` writes the S3 on `/dev/ttyACM0`. `make flash-c5` writes the C5 (`C5_PORT`, default `/dev/ttyACM1`). Confirm the port with `esptool chip-id` before flashing so the two boards are not swapped.

## Host tools

Rust stable (`cargo`, `clippy`, `rustfmt`) and `cargo-llvm-cov` for coverage.

```bash
make test
make lint
make coverage
cargo run -p espcap -- --port /dev/ttyACM0 status
```

## Firmware toolkit

Use the shared prefix `$HOME/.esp32-dev` (or `$ESP32_DEV_PREFIX`). Do not pip-install PlatformIO/esptool or clone ESP-IDF into this repo.

```bash
source "${ESP32_DEV_PREFIX:-$HOME/.esp32-dev}/activate.sh"
make firmware-s3
make firmware-c5
make flash-s3
make flash-c5
```

`flash-s3` writes USB Serial/JTAG on `/dev/ttyACM0`. After reset, the device speaks JSON lines at 115200 8N1. `espcap` deasserts RTS then DTR so Linux CDC-ACM does not reset the S3, then waits for a `status` event (up to 15s) before sending commands.

## First capture

```bash
espcap --port /dev/ttyACM0 set --radio wifi --mode discovery --format json --start
serial-capture -d /dev/ttyACM0 --json --json-nested
```

Or record with the companion CLI (holds the port until Ctrl-C; JSONL is flushed per line):

```bash
espcap --port /dev/ttyACM0 set --radio wifi --mode discovery --format json
espcap --port /dev/ttyACM0 start --json capture.jsonl
```

PCAP (companion CLI only):

```bash
espcap --port /dev/ttyACM0 set --radio wifi --mode capture --format pcap
espcap --port /dev/ttyACM0 start --pcap capture
# writes capture-wifi.pcap and capture-ble.pcap
```

ESP32-S3 rejects `--wifi-band 5`, `--wifi-band both`, and `--channels-5ghz`. Those flags are for ESP32-C5.

## CI

GitHub Actions run host/protocol `make test`, `make lint`, `make coverage`, and build the host CLI container. Firmware is not built in CI (needs the shared `~/.esp32-dev` toolkit and both chip triples). Build locally with `make firmware-s3` and `make firmware-c5`.
