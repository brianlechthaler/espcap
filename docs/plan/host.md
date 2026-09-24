# Host CLI

Package/bin name: `espcap` (`crates/host`). Native stable rustc. Does **not** use the `esp` toolchain.

[serial-capture](https://github.com/brianlechthaler/serial_capture) is receive-only. It cannot send commands. This CLI is the TX path and the PCAP recorder.

Works with **ESP32-S3 and ESP32-C5** firmware. No WROOM-specific flags or baud defaults.

## Commands (planned)

```bash
espcap --port /dev/ttyACM0 status
espcap set --radio both --mode discovery --format json --wifi-band 2.4 --dwell-ms 300 --oui F4:4E:FC --name-regex 'AirPods.*' --manufacturer-regex 'Apple'
espcap set --port /dev/ttyACM0 --wifi-band both --channels-5ghz 36,40,44,48
espcap start --json capture.jsonl
espcap start --pcap capture
espcap stop
```

`--port` required except where a later discovery flag is added. Default baud 115200. Typical device node is `/dev/ttyACM*` (USB Serial/JTAG / CDC). `/dev/ttyUSB*` is only for boards that expose a USB-UART bridge; do not assume WROOM `/dev/ttyUSB0`.

- `start --json` writes NDJSON (stdout if path is `-`).
- `start --pcap capture` writes `capture-wifi.pcap` and/or `capture-ble.pcap` from framed records.
- `--wifi-band` / `--channels-5ghz`: sent in `set`. If `status.chip` is `esp32s3` and the user asked for 5 GHz, fail on the host before sending (and firmware would `error` anyway).

## Manufacturer and OUI

Firmware does not ship IEEE OUI names. The CLI vendors a compact OUI list (CSV or generated from a pinned IEEE-derived source at build time).

`--manufacturer-regex` matches vendor names on the host, then `set` sends the corresponding OUI prefixes. `--oui` and `--mac` are sent as-is.

BLE company IDs are a separate firmware filter, not IEEE OUI names.

## serial-capture interop

JSON mode at 115200, one JSON object per `\n` line, matches serial-capture:

```bash
espcap --port /dev/ttyACM0 set --radio both --mode discovery --format json --start
serial-capture -d /dev/ttyACM0 --json --json-nested
```

`set --start` (or `start --detach`) sends `start`, waits for ack, and exits so the port is free. The USB serial port cannot be opened by both tools at once.

PCAP mode: only `espcap` should own the port.

## Dependencies (planned)

- `clap`
- `serialport`
- `serde` / `serde_json`
- `regex` (host tests and manufacturer matching)
- `pcap-file` optional for validating written files in tests; the CLI can write bytes from the protocol crate without it

## Tests

Mock serial: pty or an in-memory reader/writer. Cover:

- command JSON encoding
- ack/status parse
- `wifi_band` / `channels_5ghz` rejected for `chip=esp32s3` and accepted for `esp32c5`
- manufacturer regex → OUI list
- framed PCAP decode → two DLT files
- radiotap frequency for 2.4 vs 5 GHz
- JSONL write

No hardware required for host tests. 100% coverage on generated and changed host code.
