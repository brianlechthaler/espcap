# On-wire protocol

Default serial: **115200 8N1**. The CLI may raise baud for PCAP (for example 921600) via a `set` command and a host port reopen.

Line ending: `\n` (CRLF accepted). Commands and JSON events are one object per line so [serial-capture](https://github.com/brianlechthaler/serial_capture) can log with `--json --json-nested`.

The UART RX task always parses incoming lines, including while TX is binary PCAP, so `stop` works.

## Commands (host to device)

```json
{"cmd":"get"}
{"cmd":"set","radio":"wifi|ble|both|off","mode":"discovery|capture","format":"json|pcap","dwell_ms":300,"hopmask":"0x0421","ble_interval_ms":100,"ble_window_ms":30,"ble_active":false,"wifi_types":["mgmt","data"],"filters":{"oui":["F4:4E:FC"],"mac":["aa:bb:cc:dd:ee:ff"],"name_regex":"AirPods.*","ssid_regex":"^Flock"}}
{"cmd":"start"}
{"cmd":"stop"}
{"cmd":"status"}
```

Firmware replies with `{"event":"ack"}`, `{"event":"status",...}`, or `{"event":"error","msg":"..."}`.

Field notes:

- `radio`: `wifi`, `ble`, `both`, `off`
- `mode`: `discovery` or `capture`
- `format`: `json` or `pcap`
- `dwell_ms`: WiFi hop dwell, 100–2000, default 300
- `hopmask`: 14-bit mask, bit `(ch-1)` = channel `ch`. Default `0x0421` = channels 1, 6, 11
- `ble_interval_ms` / `ble_window_ms`: NimBLE scan timing (milliseconds)
- `ble_active`: default `false` (passive, match oui-spy BLE Sniff)
- `wifi_types`: subset of `mgmt`, `ctrl`, `data`
- `filters`: see [capture.md](capture.md)

Omitted `set` fields leave the previous value.

## JSON data mode

Compatible with `serial-capture --json --json-nested`. Every TX line is a JSON object.

Discovery events:

- `wifi_ap`
- `wifi_sta`
- `ble`

Capture events:

- `wifi_frame`
- `ble_adv`

Common fields: `event`, `mac`, `oui`, `rssi`, `channel`, `ts_ms`. Capture events add `payload_hex`. Discovery events add SSID or BLE name, `hit_count`, first/last seen as designed in the protocol crate.

Status may include drop counters: ring overflow and UART backpressure.

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

Radiotap present bits match oui-spy: TSFT, Flags, Rate, Channel, dBm Antenna Signal. Channel frequency is `2407+5*ch` (channel 14 = 2484).

BLE PHDR: channel 39 if NimBLE does not expose RF channel; reconstruct access address `D6 BE 89 8E` as in `blesniff.cpp`.

The CLI writes `out-wifi.pcap` and `out-ble.pcap` (or one file if a single radio is enabled). Classic PCAP cannot mix DLTs.

Do not stream raw unframed PCAP. Original ESP32 UART is more reliable than S3 CDC, but resync still needs a magic.

## Mixing rules

- JSON mode: all TX is NDJSON. serial-capture may record.
- PCAP mode: companion CLI owns the port. serial-capture cannot parse binary frames.
- Never mix binary PCAP and JSON on the same TX stream after `start`.
