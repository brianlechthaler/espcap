# Firmware

Primary target: original ESP32 (WROOM-32 / DevKitC), `xtensa-esp32-espidf`. Dual-core Xtensa LX6. Serial is UART0 through the board USB-UART (typically GPIO1 TX / GPIO3 RX). There is no native USB PHY.

## Toolkit

Use the shared prefix `$ESP32_DEV_PREFIX` or `$HOME/.esp32-dev`. Source `activate.sh` or wrap with `~/.cursor/skills/esp32-dev/scripts/with-env.sh`.

Do not:

- `pip install platformio` or `esptool` in this project
- `git clone` ESP-IDF into the repo
- run `rustup-init`, `espup`, or PlatformIO’s installer here

Build and flash:

```bash
source "${ESP32_DEV_PREFIX:-$HOME/.esp32-dev}/activate.sh"
cd firmware && cargo build --release
cargo espflash flash --monitor --port /dev/ttyUSB0
```

`firmware/.cargo/config.toml` (planned):

- `target = "xtensa-esp32-espidf"`
- `ESP_IDF_TOOLS_INSTALL_DIR = "fromenv"` so `esp-idf-sys` uses `IDF_PATH` from `activate.sh`
- `build-std = ["std", "panic_abort"]`
- `linker = "ldproxy"`
- runner `espflash flash --monitor`

`firmware/rust-toolchain.toml`: `channel = "esp"`.

If IDF 6.1 bindgen fails, pin `ESP_IDF_VERSION` in firmware package metadata to a tested v5.3–v6.0. Still do not clone IDF into this repo.

## sdkconfig.defaults (planned)

```
CONFIG_BT_ENABLED=y
CONFIG_BT_NIMBLE_ENABLED=y
CONFIG_BT_BLUEDROID_ENABLED=n
CONFIG_BT_NIMBLE_EXT_ADV=n
CONFIG_ESP_COEX_SW_COEXIST_ENABLE=y
CONFIG_BTDM_CTRL_FULL_SCAN_SUPPORTED=y
CONFIG_ESP_MAIN_TASK_STACK_SIZE=8192
CONFIG_BT_NIMBLE_HOST_TASK_STACK_SIZE=4096
```

Pin the Wi-Fi protocol task to core 0 and NimBLE host to core 1 (`CONFIG_ESP_WIFI_TASK_CORE_ID`, `CONFIG_BT_NIMBLE_PINNED_TO_CORE_CHOICE` / BTDM controller pin).

Original ESP32 has no BLE 5 extended advertising. Leave `CONFIG_BT_NIMBLE_EXT_ADV` off.

## Task map

| Task | Core | Job |
|------|------|-----|
| Wi-Fi driver (IDF) | 0 | Promiscuous callback copies into a ring |
| Channel hop | 0 | `esp_wifi_set_channel`, `dwell_ms` |
| NimBLE host (IDF) | 1 | `ble_gap_disc` / NimBLE scan callback enqueues ads |
| UART TX | 1 | Drain rings: JSON lines or framed PCAP |
| UART RX / commands | 1 | Parse JSON command lines; apply `set` / `start` / `stop` |

Application threads: `esp_idf_hal::cpu::ThreadSpawnConfiguration { pin_to_core: Some(Core::Core1), ... }.set()` then `std::thread::spawn`. Thread names must be NUL-terminated and at most 16 characters.

## Crates on device

- `esp-idf-svc` (and `esp-idf-sys` / `esp-idf-hal`)
- `esp32-nimble` or IDF NimBLE FFI
- `serde` / `serde_json` (or `serde-json-core` if size requires)
- `regex-lite` for SSID and name filters
- `log`

Do not depend on `pcap-file` on the chip. Hand-write classic PCAP records in the protocol crate and call that from firmware.

`esp-idf-svc` WiFi has `set_promiscuous` but filter and RX-callback wrappers may still be missing. Call `esp_wifi_set_promiscuous_rx_cb` and `esp_wifi_set_promiscuous_filter` via `esp-idf-sys`.

## RAM

ESP32-WROOM SRAM is tight (~520 KB). Keep rings bounded, drop on overflow, report drops in `status`. Do not embed the IEEE OUI database on device.

## Later chips (not v1)

ESP32-S3: PSRAM rings, native USB-CDC (oui-spy found high-rate CDC corruption; keep framing). ESP32-C3: single core, no dual-core offload.
