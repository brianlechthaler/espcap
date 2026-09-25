# CLI

`espcap` sends JSON commands over USB Serial/JTAG and records JSONL or framed PCAP.

## Overview

`--port` is required. Default baud is 115200. `make devices` prints each USB serial node and its chip (`ESP32-S3` or `ESP32-C5`). Pass that node as `--port`.

On open the CLI clears RTS, then DTR, and clears `HUPCL`, so ESP32 USB-JTAG does not reset (`RTS=1` and `DTR=0` is a chip reset). It then waits up to 15 seconds for a `status` line before sending the command.

`set` always sends `stop`, reads `status`, checks the chip, then sends `set`. A partial `set` leaves omitted fields unchanged. Firmware stores radio, mode, format, hop, BLE timing, filters, and `running` in NVS, so a later `status` from another process still sees them.

`start` holds the port until Ctrl-C. `--detach` sends `start` and exits so another process can read the port. Do not open the same USB serial node from two processes.

Manufacturer regex is resolved on the host against a vendored OUI list and sent as OUI prefixes. Firmware does not store IEEE names. A pattern that matches nothing is an error.

## Usage

```bash
espcap --port /dev/ttyACM0 status
espcap --port /dev/ttyACM0 set --radio both --mode discovery --format json --oui F4:4E:FC
espcap --port /dev/ttyACM0 set --manufacturer-regex Apple
espcap --port /dev/ttyACM0 start --json capture.jsonl
espcap --port /dev/ttyACM0 start --pcap capture
espcap --port /dev/ttyACM0 stop
```

`--json -` writes JSONL to stdout. `--pcap capture` writes `capture-wifi.pcap` (DLT 127) and `capture-ble.pcap` (DLT 256). Both files are created even when one radio is off.

For [serial-capture](https://github.com/brianlechthaler/serial_capture), configure and release the port in one process:

```bash
espcap --port /dev/ttyACM0 set --radio wifi --mode discovery --format json --start
serial-capture -d /dev/ttyACM0 --json --json-nested
```

If the device is already running: `espcap --port /dev/ttyACM0 start --detach`.

PCAP is binary. serial-capture only applies to JSON lines.

## Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--port` | required | Serial device. |
| `--baud` | `115200` | Passed to the serial driver. USB Serial/JTAG ignores baud. |
| `--radio` | unchanged (`off` on a fresh device) | `wifi`, `ble`, `both`, `off`. |
| `--mode` | unchanged (`discovery`) | `discovery` or `capture`. |
| `--format` | unchanged (`json`) | `json` or `pcap`. |
| `--wifi-band` | unchanged (`2.4`) | `2.4`, `5`, `both`. |
| `--dwell-ms` | unchanged (`300`) | 100–2000. |
| `--hopmask` | unchanged (`0x0421`) | Hex, optional `0x` prefix. |
| `--channels-5ghz` | unchanged | Comma-separated channel numbers. |
| `--ble-interval-ms` | unchanged (`100`) | Scan interval. |
| `--ble-window-ms` | unchanged (`30`) | Scan window. |
| `--ble-active` | unchanged (`false`) | `true` or `false`. |
| `--wifi-types` | unchanged (`mgmt,data`) | Comma-separated `mgmt`, `ctrl`, `data`. |
| `--oui` | none | Comma-separated OUI prefixes. Replaces the OUI list when set. |
| `--mac` | none | Comma-separated full MAC addresses. |
| `--ssid-regex` | none | SSID must match. |
| `--name-regex` | none | BLE local name must match. |
| `--company-id` | none | Comma-separated company IDs. |
| `--manufacturer-regex` | none | Host-side. Matching OUIs are added to `--oui`. |
| `--start` | `false` | On `set`, send `start` after `ack`. |

Filter fields combine with AND. An empty filter matches everything. Setting filters replaces the previous filter spec.

## Troubleshooting

| Symptom | Cause |
|---------|--------|
| Timeout waiting for status | Wrong port, or the board reset because RTS/DTR was left asserted by another tool. |
| `ESP32-S3 does not support 5 GHz` | `--wifi-band 5`, `--wifi-band both`, or `--channels-5ghz` on an S3. |
| `no OUI matched manufacturer regex` | The vendored list has no name matching the pattern. |
| `start` and serial-capture both fail | Both opened the same port. Use `--start` on `set`, or `start --detach`. |

## Related

- [Getting started](../getting-started.md)
- [WiFi](wifi.md)
- [BLE](ble.md)
- [Protocol](protocol.md)
