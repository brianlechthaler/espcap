# Capture behavior

Model WiFi and BLE capture after [oui-spy-unified-blue](https://github.com/colonelpanichacks/oui-spy-unified-blue), not after `WiFi.scanNetworks()`.

## Discovery vs capture

**Discovery** (device and AP inventory; Flock-You / Detector style):

- WiFi: Beacon, Probe Request, Probe Response only. Extract BSSID / addr2 / addr3, SSID, RSSI, channel, frequency, frame subtype. Dedupe by MAC. Emit on first seen and on cooldown or RSSI change.
- BLE: advertisements, passive scan, duplicates on. Extract addr, addr_type, RSSI, local name, company ID, service UUIDs, TxPower if present.
- Default hop: channels **1, 6, 11**, dwell **300 ms** (configurable 100–2000 ms). Flock-You’s 11→6→1 at 250 ms is an optional hop profile, not the default.

**Capture** (general traffic; oui-spy PCAP mode):

- WiFi: MGMT / CTRL / DATA bitmask. Default MGMT+DATA; CTRL off unless enabled.
- Optional SoftAP-locked single channel vs STA/NULL hop. Hop tears down a SoftAP (same tradeoff as oui-spy).
- BLE: advertisement PDUs only. Reconstruct DLT 256. No connected LL data.
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

Control frames need `WIFI_PROMIS_FILTER_MASK_CTRL` and `esp_wifi_set_promiscuous_ctrl_filter`.

SSID from tagged IEs: Beacon / Probe Response at offset 24+12, Probe Request at offset 24.

## BLE scan (oui-spy BLE Sniff / Detector)

NimBLE GAP observer. Controller hops advertising channels 37/38/39 internally. Firmware does not set BLE RF channel.

v1: **passive** scan (`setActiveScan(false)` / `passive=1`), duplicates on, `start` forever. Match BLE Sniff, not Detector’s active SCAN_REQ.

Typical timing: interval 100 ms, window 30 ms when WiFi coexistence is on; interval == window when BLE-only or `BOTH` with locked WiFi channel.

Do not use Classic BR/EDR inquiry.

## Hop profiles

- Default mask `0x0421` = channels 1, 6, 11.
- Optional Flock-You profile `{11, 6, 1}` at 250 ms dwell.
- Dwell 100–2000 ms, default 300 (oui-spy PCAP default).
- Hop task on core 0. `esp_wifi_set_channel` only; no Serial from that path.

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
