# Host CLI

Package/bin name: `espcap` (`crates/host`). Native stable rustc. Does **not** use the `esp` toolchain.

[serial-capture](https://github.com/brianlechthaler/serial_capture) is receive-only. It cannot send commands. This CLI is the TX path and the PCAP recorder.

## Commands (planned)

```bash
espcap --port /dev/ttyUSB0 status
espcap set --radio both --mode discovery --format json --dwell-ms 300 --oui F4:4E:FC --name-regex 'AirPods.*' --manufacturer-regex 'Apple'
espcap start --json capture.jsonl
espcap start --pcap capture
espcap stop
```

`--port` required except where a later discovery flag is added. Default baud 115200.

- `start --json` writes NDJSON (stdout if path is `-`).
- `start --pcap capture` writes `capture-wifi.pcap` and/or `capture-ble.pcap` from framed records.

## Manufacturer and OUI

Firmware does not ship IEEE OUI names. The CLI vendors a compact OUI list (CSV or generated from a pinned IEEE-derived source at build time).

`--manufacturer-regex` matches vendor names on the host, then `set` sends the corresponding OUI prefixes. `--oui` and `--mac` are sent as-is.

BLE company IDs are a separate firmware filter, not IEEE OUI names.

## serial-capture interop

JSON mode at 115200, one JSON object per `\n` line, matches serial-capture:

```bash
espcap set --port /dev/ttyUSB0 --format json --mode discovery --radio both
espcap start --port /dev/ttyUSB0
# then, after closing espcap so the port is free:
serial-capture -d /dev/ttyUSB0 --json --json-nested
```

The USB serial port cannot be opened by both tools at once. Document that.

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
- manufacturer regex → OUI list
- framed PCAP decode → two DLT files
- JSONL write

No hardware required for host tests. 100% coverage on generated and changed host code.
