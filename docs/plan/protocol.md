# On-wire protocol

Default serial: **115200 8N1** on USB CDC / USB Serial-JTAG (`/dev/ttyACM*` on typical S3 and C5 boards). The CLI may raise baud for PCAP (for example 921600) via a `set` command and a host port reopen. Native USB CDC often ignores baud; keep the field anyway so UART fallback stays consistent.

Line ending: `\n` (CRLF accepted). Commands and JSON events are one object per line so [serial-capture](https://github.com/brianlechthaler/serial_capture) can log with `--json --json-nested`.

The serial RX task always parses incoming lines, including while TX is binary PCAP, so `stop` works.

## Commands (host to device)

```json
{"cmd":"get"}
{"cmd":"set","radio":"wifi|ble|both|off","mode":"discovery|capture","format":"json|pcap","wifi_band":"2.4|5|both","dwell_ms":300,"hopmask":"0x0421","channels_5ghz":[36,40,44,48],"ble_interval_ms":100,"ble_window_ms":30,"ble_active":false,"wifi_types":["mgmt","data"],"filters":{"oui":["F4:4E:FC"],"mac":["aa:bb:cc:dd:ee:ff"],"name_regex":"AirPods.*","ssid_regex":"^Flock"}}
{"cmd":"start"}
{"cmd":"stop"}
{"cmd":"status"}
```

Firmware replies with `{"event":"ack"}`, `{"event":"status",...}`, or `{"event":"error","msg":"..."}`.

Field notes:

- `radio`: `wifi`, `ble`, `both`, `off`
- `mode`: `discovery` or `capture`
- `format`: `json` or `pcap`
- `wifi_band`: `2.4`, `5`, or `both`. Default `2.4`. **ESP32-S3** accepts `2.4` only; `5` or `both` → `error`. **ESP32-C5** accepts all three.
- `dwell_ms`: WiFi hop dwell, 100–2000, default 300
- `hopmask`: 14-bit mask for **2.4 GHz** channels 1–14, bit `(ch-1)` = channel `ch`. Default `0x0421` = channels 1, 6, 11. Ignored when `wifi_band` is `5`.
- `channels_5ghz`: IEEE 5 GHz channel numbers (for example UNII-1 `36,40,44,48`). Default empty (no 5 GHz hop). C5 only; S3 → `error` if non-empty.
- `ble_interval_ms` / `ble_window_ms`: NimBLE scan timing (milliseconds)
- `ble_active`: default `false` (passive, match oui-spy BLE Sniff)
- `wifi_types`: subset of `mgmt`, `ctrl`, `data`
- `filters`: see [capture.md](capture.md)

Omitted `set` fields leave the previous value.

`status` includes `chip` (`esp32s3` | `esp32c5`) and `wifi_bands` (`["2.4"]` or `["2.4","5"]`) so the CLI can validate before `set`.

## JSON data mode

Compatible with `serial-capture --json --json-nested`. Every TX line is a JSON object.

Discovery events:

- `wifi_ap`
- `wifi_sta`
- `ble`

Capture events:

- `wifi_frame`
- `ble_adv`

Common fields: `event`, `mac`, `oui`, `rssi`, `channel`, `freq_mhz`, `ts_ms`. Capture events add `payload_hex`. Discovery events add SSID or BLE name, `hit_count`, first/last seen as designed in the protocol crate.

`channel` is the IEEE channel number (2.4 or 5 GHz). `freq_mhz` is the actual center frequency so 5 GHz is not stuffed into a 2.4 GHz formula.

Status may include drop counters: ring overflow, UART/CDC backpressure, and C5 truncated MIMO/HE length-only dumps.

## PCAP mode (exclusive)

After a successful `start` ack, firmware TX is **only** framed records. No mixed log lines.

Frame layout:

```
0xA5 0x5A | type_u8 | len_u16be | payload
```

| type | payload |
|------|---------|
| 0 | Classic PCAP global header once (magic `0xA1B2C3D4`, v2.4, snaplen 2500) |
| 1 | 16-byte pcap record + 23-byte radiotap + 802.11 MPDU. Host file DLT **127** |
| 2 | 16-byte pcap record + 10-byte BTLE_RF PHDR + reconstructed ADV PDU. Host file DLT **256** |

Radiotap present bits match oui-spy: TSFT, Flags, Rate, Channel, dBm Antenna Signal.

Channel frequency in radiotap:

- 2.4 GHz: `2407 + 5 * ch` (channel 14 = 2484)
- 5 GHz: actual MHz for that IEEE channel (typically `5000 + 5 * ch` for 36–165; do not use the 2.4 formula)

BLE PHDR: channel 39 if NimBLE does not expose RF channel; reconstruct access address `D6 BE 89 8E` as in `blesniff.cpp`.

The CLI writes `out-wifi.pcap` and `out-ble.pcap` (or one file if a single radio is enabled). Classic PCAP cannot mix DLTs.

Do not stream raw unframed PCAP. S3/C5 native CDC is less reliable at high rate than a USB-UART bridge; resync still needs a magic.

## Mixing rules

- JSON mode: all TX is NDJSON. serial-capture may record.
- PCAP mode: companion CLI owns the port. serial-capture cannot parse binary frames.
- Never mix binary PCAP and JSON on the same TX stream after `start`.
