# Capture behavior

Model WiFi and BLE capture after [oui-spy-unified-blue](https://github.com/colonelpanichacks/oui-spy-unified-blue), not after `WiFi.scanNetworks()`.

## Discovery vs capture

**Discovery** (device and AP inventory; Flock-You / Detector style):

- WiFi: Beacon, Probe Request, Probe Response only. Extract BSSID / addr2 / addr3, SSID, RSSI, channel, frequency, frame subtype. Dedupe by MAC. Emit on first seen and on cooldown or RSSI change.
- BLE: advertisements, passive scan, duplicates on. Extract addr, addr_type, RSSI, local name, company ID, service UUIDs, TxPower if present. Extended advertising PDUs are in scope (both chips support BLE 5 scan).
- Default hop: 2.4 GHz channels **1, 6, 11**, dwell **300 ms** (configurable 100–2000 ms). Flock-You’s 11→6→1 at 250 ms is an optional hop profile, not the default.
- C5 5 GHz discovery: only when `wifi_band` is `5` or `both` and `channels_5ghz` is non-empty. Default 5 GHz list is UNII-1 **36, 40, 44, 48**. Do not hop DFS channels unless the user sets them.

**Capture** (general traffic; oui-spy PCAP mode):

- WiFi: MGMT / CTRL / DATA bitmask. Default MGMT+DATA; CTRL off unless enabled.
- Optional SoftAP-locked single channel vs STA/NULL hop. Hop tears down a SoftAP (same tradeoff as oui-spy).
- BLE: advertisement PDUs only (including extended ads when NimBLE delivers them). Reconstruct DLT 256. No connected LL data.
- Output every matching frame after filters, not deduped devices.

## WiFi promiscuous (oui-spy PCAP / Flock-You)

Prefer `WIFI_MODE_NULL` or unconnected STA. Do not associate if hopping.

```
esp_wifi_set_promiscuous_filter(...)
esp_wifi_set_promiscuous_rx_cb(...)
esp_wifi_set_promiscuous(true)
esp_wifi_set_channel(ch, WIFI_SECOND_CHAN_NONE)
```

Packet type: `wifi_promiscuous_pkt_t` → `rx_ctrl.sig_len`, `rx_ctrl.rssi`, `rx_ctrl.channel`, `rx_ctrl.rate`, `payload[]` (802.11 MAC frame). `sig_len` includes FCS; clamp copy length.

On **C5**, also copy band/frequency from `rx_ctrl` (IDF fields for 5 GHz). HE/MIMO frames that the sniffer only reports as a length are not stored as fake MPDUs.

Control frames need `WIFI_PROMIS_FILTER_MASK_CTRL` and `esp_wifi_set_promiscuous_ctrl_filter`.

SSID from tagged IEs: Beacon / Probe Response at offset 24+12, Probe Request at offset 24.

## BLE scan (oui-spy BLE Sniff / Detector)

NimBLE GAP observer. Controller hops advertising channels 37/38/39 internally. Firmware does not set BLE RF channel.

v1: **passive** scan (`setActiveScan(false)` / `passive=1`), duplicates on, `start` forever. Match BLE Sniff, not Detector’s active SCAN_REQ.

Typical timing: interval 100 ms, window 30 ms when WiFi coexistence is on; interval == window when BLE-only or `BOTH` with locked WiFi channel.

Do not use Classic BR/EDR inquiry. Do not use C5 IEEE 802.15.4.

## Hop profiles

- Default 2.4 GHz mask `0x0421` = channels 1, 6, 11. Used on **S3 and C5**.
- Optional Flock-You profile `{11, 6, 1}` at 250 ms dwell.
- Dwell 100–2000 ms, default 300 (oui-spy PCAP default).
- **S3:** hop task on core 0. 2.4 GHz only. `esp_wifi_set_channel` only; no Serial from that path. Reject 5 GHz `set`.
- **C5:** hop task on the HP core. When `wifi_band` is `both`, interleave 2.4 hopmask channels with `channels_5ghz` (one channel per dwell). When `5`, hop only `channels_5ghz`. Default 5 GHz list UNII-1 `{36, 40, 44, 48}`.
- Default DFS policy: off. If the user includes DFS channel numbers, hop them in sniffer/NULL mode (no AP beacon on DFS).

## Filters

Evaluated on firmware after parse, before encode. Empty lists pass everything.

- **OUI** allowlist: first 3 octets of addr2 (WiFi) or BLE addr.
- **Full MAC** allowlist.
- **SSID regex** and **BLE local-name regex**: `regex-lite` on firmware; host tests use the same patterns.
- **BLE company ID** allowlist (for example `0x004C`).
- **Manufacturer name regex**: not on device. Host CLI resolves names against a vendored OUI list and pushes OUI prefixes. See [host.md](host.md).

## Mapping to oui-spy modes

| espcap mode | Closest oui-spy mode | What to copy |
|-------------|----------------------|--------------|
| Discovery + WiFi | Flock-You promiscuous | NULL mode, MGMT parse, hop, OUI match, no TX |
| Discovery + BLE | Detector / BLE Sniff | NimBLE ads, MAC/OUI/name/CID |
| Capture + WiFi | PCAP | Promisc filter, radiotap DLT 127, hop or lock |
| Capture + BLE | BLE Sniff | Passive ads, DLT 256 reconstruct |
