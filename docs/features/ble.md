# BLE advertisements

Passive advertising scan. Discovery would emit deduplicated `ble` JSON. Capture would emit `ble_adv` JSON or DLT 256 PCAP rebuilt from the advertising PDU.

## Overview

No Classic BR/EDR and no connections. The host and the on-wire protocol accept BLE settings. This firmware build does not start NimBLE: `spawn_ble` drops the packet channel, and `apply_radios` logs that the NimBLE host is unavailable. `radio` `ble` or `both` is stored and returned by `status`, but no `ble` or `ble_adv` lines are produced.

The event path that exists in the protocol crate is described below so a later scan task can match it.

Scan interval and window come from `set --ble-interval-ms` and `--ble-window-ms` (defaults 100 ms and 30 ms). `--ble-active true` is the flag for active scan requests. The default is passive (`false`).

Discovery dedup is per address, with a 5 second cooldown. There is no RSSI-jump exception (WiFi has one). `channel` and `freq_mhz` on BLE events are fixed at 39 and 2480. `addr_type` is `public` or `random`.

Name and company ID are parsed from AD structures: types `0x08` (short name) and `0x09` (complete name, preferred), and manufacturer data `0xFF` (first two bytes, little-endian company ID).

Capture is not deduped. PCAP uses DLT 256 (`BLUETOOTH_LE_LL_WITH_PHDR`): 10-byte RF header, access address `0x8E89BED6`, then the advertising PDU.

## Usage

Settings are accepted and saved in NVS. They do not start a scan on this build:

```bash
espcap --port /dev/ttyACM0 set --radio ble --mode discovery --format json \
  --ble-interval-ms 100 --ble-window-ms 30 --ble-active false
espcap --port /dev/ttyACM0 status
```

A discovery line, once a scan task exists, looks like:

```json
{"event":"ble","mac":"aa:bb:cc:dd:ee:ff","oui":"AA:BB:CC","rssi":-60,"channel":39,"freq_mhz":2480,"ts_ms":12,"name":"AirPods","company_id":76,"hit_count":1,"first_ts_ms":12,"last_ts_ms":12,"addr_type":"random"}
```

`name` and `company_id` are omitted when the advertisement has neither.

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `ble_interval_ms` | `100` | Scan interval, milliseconds. |
| `ble_window_ms` | `30` | Scan window, milliseconds. |
| `ble_active` | `false` | `true` requests active scanning. |
| `name_regex` | none | Local name must match. |
| `company_id` | none | Company-ID allowlist (comma-separated on the CLI). |
| `oui` / `mac` | none | Same allowlists as WiFi. Applied to the advertiser address. |

Empty filters pass every advertisement that a scan task enqueues.

## Troubleshooting

| Symptom | Cause |
|---------|--------|
| `status` shows `radio` `ble` and no advertisements | Expected on this build. NimBLE is not started. |
| `capture-ble.pcap` is only a global header | The CLI always creates the BLE file. No scan means no records. |

## Related

- [CLI](cli.md)
- [WiFi](wifi.md)
- [Protocol](protocol.md)
