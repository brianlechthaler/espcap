# Firmware

Two first-class targets, one crate:

| Chip | Rust target | IDF MCU | Serial |
|------|-------------|---------|--------|
| ESP32-S3 | `xtensa-esp32s3-espidf` | `esp32s3` | USB Serial/JTAG or USB-OTG CDC (`/dev/ttyACM*`); UART0 GPIO43/44 only as fallback |
| ESP32-C5 | `riscv32imac-esp-espidf` | `esp32c5` | USB Serial/JTAG (`/dev/ttyACM*`) |

Do **not** add original ESP32 / WROOM (`xtensa-esp32-espidf`). There is no hardware to flash or test.

S3: dual-core Xtensa LX7, 2.4 GHz WiFi, BLE 5, optional PSRAM. C5: single HP RISC-V (240 MHz), dual-band Wi-Fi 6, BLE 5, 384 KB HP SRAM, optional PSRAM.

## Toolkit

Use the shared prefix `$ESP32_DEV_PREFIX` or `$HOME/.esp32-dev`. Source `activate.sh` or wrap with `~/.cursor/skills/esp32-dev/scripts/with-env.sh`.

Do not:

- `pip install platformio` or `esptool` in this project
- `git clone` ESP-IDF into the repo
- run `rustup-init`, `espup`, or PlatformIO’s installer here

Build and flash:

```bash
source "${ESP32_DEV_PREFIX:-$HOME/.esp32-dev}/activate.sh"
cd firmware

# ESP32-S3
MCU=esp32s3 cargo build --release --target xtensa-esp32s3-espidf
MCU=esp32s3 cargo espflash flash --target xtensa-esp32s3-espidf --monitor --port /dev/ttyACM0

# ESP32-C5
MCU=esp32c5 cargo build --release --target riscv32imac-esp-espidf
MCU=esp32c5 cargo espflash flash --target riscv32imac-esp-espidf --monitor --port /dev/ttyACM0
```

Makefile wrappers (planned): `make firmware-s3`, `make firmware-c5`. Never a WROOM/`esp32` target.

`firmware/.cargo/config.toml` (planned):

- **No** default `build.target` — S3 and C5 triples are different; make recipes pass `--target`
- `[target.xtensa-esp32s3-espidf]` and `[target.riscv32imac-esp-espidf]`: `linker = "ldproxy"`, runner `espflash flash --monitor`
- `ESP_IDF_TOOLS_INSTALL_DIR = "fromenv"` so `esp-idf-sys` uses `IDF_PATH` from `activate.sh`
- `build-std = ["std", "panic_abort"]`

`firmware/rust-toolchain.toml`: `channel = "esp"` (covers Xtensa S3 and RISC-V C5).

C5 needs IDF **5.5+** (or 6.x in the shared toolkit). If bindgen fails, pin `ESP_IDF_VERSION` in firmware package metadata to a version that supports both chips. Still do not clone IDF into this repo.

## sdkconfig.defaults (planned, shared)

```
CONFIG_BT_ENABLED=y
CONFIG_BT_NIMBLE_ENABLED=y
CONFIG_BT_BLUEDROID_ENABLED=n
CONFIG_BT_NIMBLE_EXT_ADV=y
CONFIG_ESP_COEX_SW_COEXIST_ENABLE=y
CONFIG_ESP_MAIN_TASK_STACK_SIZE=8192
CONFIG_BT_NIMBLE_HOST_TASK_STACK_SIZE=4096
```

Both chips can scan BLE 5 extended advertisements. Enable `CONFIG_BT_NIMBLE_EXT_ADV` for **observer/scan**, not as a reason to implement BLE connections.

USB console (planned, both chips): route stdio / protocol to USB Serial/JTAG (`CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG` or CDC). Do not assume an external USB-UART bridge.

### sdkconfig.defaults.esp32s3

Pin the Wi-Fi protocol task to core 0 and NimBLE host to core 1 (`CONFIG_ESP_WIFI_TASK_CORE_ID`, `CONFIG_BT_NIMBLE_PINNED_TO_CORE_CHOICE`). Enable PSRAM in sdkconfig only when the board actually has it; do not put atomics in PSRAM.

### sdkconfig.defaults.esp32c5

Do not pin Wi-Fi / NimBLE to a second core. Keep coexistence on. Enable 5 GHz / dual-band as required by the IDF C5 WiFi driver. PSRAM only if the module has it.

## Task map

### ESP32-S3

| Task | Core | Job |
|------|------|-----|
| Wi-Fi driver (IDF) | 0 | Promiscuous callback copies into a ring |
| Channel hop | 0 | `esp_wifi_set_channel`, `dwell_ms` (2.4 GHz only) |
| NimBLE host (IDF) | 1 | `ble_gap_disc` / NimBLE scan callback enqueues ads |
| Serial TX | 1 | Drain rings: JSON lines or framed PCAP |
| Serial RX / commands | 1 | Parse JSON command lines; apply `set` / `start` / `stop` |

Application threads: `esp_idf_hal::cpu::ThreadSpawnConfiguration { pin_to_core: Some(Core::Core1), ... }.set()` then `std::thread::spawn`. Thread names must be NUL-terminated and at most 16 characters.

### ESP32-C5

| Task | Core | Job |
|------|------|-----|
| Wi-Fi driver (IDF) | HP | Promiscuous callback copies into a ring |
| Channel hop | HP | `esp_wifi_set_channel` for 2.4 and 5 GHz |
| NimBLE host (IDF) | HP | `ble_gap_disc` / scan callback enqueues ads |
| Serial TX | HP | Drain rings: JSON lines or framed PCAP |
| Serial RX / commands | HP | Parse JSON command lines; apply `set` / `start` / `stop` |

Do not call `pin_to_core: Some(Core::Core1)` on C5. Same copy-only callback rule; encode on a dedicated app thread so the driver task stays short.

## Crates on device

- `esp-idf-svc` (and `esp-idf-sys` / `esp-idf-hal`)
- `esp32-nimble` or IDF NimBLE FFI
- `serde` / `serde_json` (or `serde-json-core` if size requires)
- `regex-lite` for SSID and name filters
- `log`

Do not depend on `pcap-file` on the chip. Hand-write classic PCAP records in the protocol crate and call that from firmware.

`esp-idf-svc` WiFi has `set_promiscuous` but filter and RX-callback wrappers may still be missing. Call `esp_wifi_set_promiscuous_rx_cb` and `esp_wifi_set_promiscuous_filter` via `esp-idf-sys`.

## RAM

Keep rings bounded, drop on overflow, report drops in `status`. Do not embed the IEEE OUI database on device.

- **S3:** Prefer on-chip SRAM for rings. If the board has PSRAM, extra ring capacity is allowed; never place atomics in PSRAM.
- **C5:** 384 KB HP SRAM is tighter than S3. Default rings stay small; PSRAM only when the module has it.

## Serial reliability

Both chips talk to the host over native USB CDC / USB Serial-JTAG. High-rate CDC can corrupt streams (oui-spy). Always use the framed PCAP magic and JSON-one-object-per-line; never raw unframed PCAP.
