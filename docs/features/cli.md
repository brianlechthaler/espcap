# CLI

`espcap` sends JSON commands and records JSONL or framed PCAP from firmware.

```bash
espcap --port /dev/ttyACM0 status
espcap --port /dev/ttyACM0 set --radio both --mode discovery --format json --oui F4:4E:FC
espcap --port /dev/ttyACM0 set --manufacturer-regex Apple
espcap --port /dev/ttyACM0 start --json capture.jsonl
espcap --port /dev/ttyACM0 start --pcap capture
espcap --port /dev/ttyACM0 stop
```

`--port` is required. Default baud is 115200. Typical device node is `/dev/ttyACM*`.

Manufacturer regex is resolved on the host against a vendored OUI list and sent as OUI prefixes. Firmware does not ship IEEE names.
