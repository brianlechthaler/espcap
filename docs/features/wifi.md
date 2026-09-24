# WiFi capture

Discovery parses Beacon, Probe Request, and Probe Response, then emits `wifi_ap` / `wifi_sta` JSON with dedup. Capture emits every matching MGMT/CTRL/DATA frame as `wifi_frame` or radiotap PCAP (DLT 127).

Default hop is 2.4 GHz channels 1, 6, 11 at 300 ms dwell (`hopmask` `0x0421`). ESP32-S3 is 2.4 GHz only. ESP32-C5 may hop 5 GHz when `wifi_band` is `5` or `both`.
