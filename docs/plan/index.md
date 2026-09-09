# espcap implementation plan

Greenfield Rust workspace: ESP32 (original dual-core) firmware that sniffs WiFi in promiscuous mode and BLE advertisements, plus a host CLI that configures the device and records JSONL or PCAP.

Capture behavior is modeled on [oui-spy-unified-blue](https://github.com/colonelpanichacks/oui-spy-unified-blue) (`pcap.cpp`, `blesniff.cpp`, `flockyou_promiscious.cpp`), not the hub README alone.

Promiscuous capture of other people's traffic may be restricted. Use this tool only on networks and devices you are authorized to monitor.

## Locked decisions

- Dual on-wire modes: newline JSON (compatible with [serial-capture](https://github.com/brianlechthaler/serial_capture)) **and** exclusive framed binary PCAP (companion CLI only).
- Radios for v1: **WiFi promiscuous + BLE advertisements only**. No Classic Bluetooth.
- Primary chip: **ESP32 (WROOM-32 / DevKitC)**, target `xtensa-esp32-espidf`, both Xtensa cores.
- **No ESP8266 firmware.** No BLE, no viable Rust WiFi sniffer, `esp8266-hal` archived. WiFi JSON fields stay radio-agnostic so a later C target could reuse the protocol.
- Stack: **esp-idf-svc (std)** + NimBLE, not embassy / `esp-hal`. Reuse `~/.esp32-dev` via `activate.sh` / `with-env.sh`. Do not pip-install PlatformIO or esptool, and do not clone ESP-IDF into this repo.

## Plan docs

| Doc | Contents |
|-----|----------|
| [architecture.md](architecture.md) | Cores, rings, coexistence, crate layout |
| [protocol.md](protocol.md) | Serial commands, JSON events, PCAP framing |
| [capture.md](capture.md) | Discovery vs capture, hop, filters, oui-spy mapping |
| [firmware.md](firmware.md) | SDK config, toolkit, UART, task map |
| [host.md](host.md) | `espcap` CLI, manufacturer-to-OUI, serial-capture interop |
| [phases.md](phases.md) | TDD/lint gates and implementation order |

## Out of scope (v1)

- ESP8266 / C3 / S3 firmware targets (S3 is the later upgrade: PSRAM + USB-CDC)
- Classic Bluetooth inquiry, BLE connections, 5 GHz / ESP32-C5
- Web dashboard, MQTT, SPIFFS session files (oui-spy extras)
- Changes to serial-capture (stay compatible only)
