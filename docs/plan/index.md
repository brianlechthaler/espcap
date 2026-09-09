# espcap implementation plan

Greenfield Rust workspace: ESP32-S3 and ESP32-C5 firmware that sniffs WiFi in promiscuous mode and BLE advertisements, plus a host CLI that configures the device and records JSONL or PCAP.

Capture behavior is modeled on [oui-spy-unified-blue](https://github.com/colonelpanichacks/oui-spy-unified-blue) (`pcap.cpp`, `blesniff.cpp`, `flockyou_promiscious.cpp`), not the hub README alone.

Promiscuous capture of other people's traffic may be restricted. Use this tool only on networks and devices you are authorized to monitor.

## Locked decisions

- Dual on-wire modes: newline JSON (compatible with [serial-capture](https://github.com/brianlechthaler/serial_capture)) **and** exclusive framed binary PCAP (companion CLI only).
- Radios for v1: **WiFi promiscuous + BLE advertisements only**. No Classic Bluetooth. No 802.15.4 / Zigbee / Thread (the C5 radio is unused for that).
- Hardware for v1: **ESP32-S3** (`xtensa-esp32s3-espidf`) **and ESP32-C5** (`riscv32imac-esp-espidf`). These are the boards on hand. **No original ESP32 / WROOM firmware** and no other Espressif chips.
- WiFi bands: S3 is **2.4 GHz only**. C5 is **2.4 GHz and 5 GHz**. Host `set` that asks the S3 for 5 GHz gets an `error` event.
- Stack: **esp-idf-svc (std)** + NimBLE, not embassy / `esp-hal`. Reuse `~/.esp32-dev` via `activate.sh` / `with-env.sh`. Do not pip-install PlatformIO or esptool, and do not clone ESP-IDF into this repo.

## Plan docs

| Doc | Contents |
|-----|----------|
| [architecture.md](architecture.md) | Cores, rings, coexistence, crate layout |
| [protocol.md](protocol.md) | Serial commands, JSON events, PCAP framing |
| [capture.md](capture.md) | Discovery vs capture, hop, filters, oui-spy mapping |
| [firmware.md](firmware.md) | SDK config, toolkit, USB serial, per-chip task maps |
| [host.md](host.md) | `espcap` CLI, manufacturer-to-OUI, serial-capture interop |
| [phases.md](phases.md) | TDD/lint gates and implementation order |

## Out of scope (v1)

- Original ESP32 (WROOM-32 / DevKitC), ESP8266, C2, C3, C6, C61, S2, H2, P4
- Classic Bluetooth inquiry, BLE connections, 802.15.4
- Web dashboard, MQTT, SPIFFS session files (oui-spy extras)
- Changes to serial-capture (stay compatible only)
