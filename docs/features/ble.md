# BLE advertisements

Passive NimBLE scan (no Classic BR/EDR, no connections). Discovery emits `ble` JSON; capture emits `ble_adv` or DLT 256 PCAP reconstructed from advertising PDUs.

Both ESP32-S3 and ESP32-C5 run the same observer. Extended ads (`BLE_GAP_EVENT_EXT_DISC`) are parsed when NimBLE reports them. Scan interval/window come from `set --ble-interval-ms` / `--ble-window-ms` (defaults 100 ms / 30 ms). `--ble-active true` sends scan requests; the default is passive.

Hardware: BLE is exercised on the attached ESP32-S3. ESP32-C5 firmware builds with `make firmware-c5` but is not flashed until that board is connected.
