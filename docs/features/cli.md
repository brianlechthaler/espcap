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

`--port` is required. Default baud is 115200. Typical device node is `/dev/ttyACM*`. Opening the port may reset USB-JTAG; the CLI waits for a `status` JSON before sending commands and does not leave DTR asserted.

`start` holds the port and streams JSONL as lines arrive (stdout if `--json -`). For [serial-capture](https://github.com/brianlechthaler/serial_capture), configure and detach in one process so the port is free:

```bash
espcap --port /dev/ttyACM0 set --radio wifi --mode discovery --format json --start
serial-capture -d /dev/ttyACM0 --json --json-nested
```

Or, if the device is already configured: `espcap --port /dev/ttyACM0 start --detach`. Do not open the USB serial port from both tools at once.

Manufacturer regex is resolved on the host against a vendored OUI list and sent as OUI prefixes. Firmware does not ship IEEE names.
