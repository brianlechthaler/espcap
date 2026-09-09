# Architecture

See the implementation plan in [plan/architecture.md](plan/architecture.md) for radio/core mapping.

```mermaid
flowchart LR
  Wifi[WiFi_promisc] --> Ring[rings]
  Ble[BLE_scan] --> Ring
  Ring --> Filter --> Enc[JSON_or_PCAP]
  Enc --> USB[USB_Serial_JTAG]
  CLI[espcap_CLI] --> USB
```

Shared types live in `crates/protocol`. The host CLI is `crates/host`. Firmware is a separate crate in `firmware/` built twice (`xtensa-esp32s3-espidf`, `riscv32imac-esp-espidf`).
