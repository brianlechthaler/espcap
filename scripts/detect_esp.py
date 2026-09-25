#!/usr/bin/env python3
"""Find the serial port for an attached ESP32-S3 or ESP32-C5."""

import contextlib
import glob
import sys

CHIP_NAMES = {
    "esp32s3": "ESP32-S3",
    "esp32c5": "ESP32-C5",
}


def chip_name(key):
    return CHIP_NAMES.get(key)


def choose(want, found):
    hits = [port for port, name in found if name == want]
    if len(hits) == 1:
        return hits[0]
    if not hits:
        seen = ", ".join(f"{port}={name}" for port, name in found) or "none"
        raise SystemExit(f"no {want} found ({seen})")
    raise SystemExit(f"multiple {want}: {', '.join(hits)}; set S3_PORT or C5_PORT")


def serial_ports(globber=glob.glob):
    ports = []
    for pattern in ("/dev/ttyACM*", "/dev/ttyUSB*"):
        ports.extend(globber(pattern))
    return sorted(set(ports))


def identify(port, detect_chip):
    try:
        with contextlib.redirect_stdout(sys.stderr):
            chip = detect_chip(port, connect_attempts=1)
    except Exception:
        return None
    try:
        return chip.CHIP_NAME
    finally:
        with contextlib.redirect_stdout(sys.stderr):
            try:
                chip.hard_reset()
            except Exception:
                pass
            try:
                chip._port.close()
            except Exception:
                pass


def scan(ports, detect_chip):
    found = []
    for port in ports:
        name = identify(port, detect_chip)
        if name:
            found.append((port, name))
    return found


def main(argv, detect_chip, ports):
    if len(argv) not in (2, 3) or argv[1] in ("-h", "--help"):
        raise SystemExit("usage: detect_esp.py esp32s3|esp32c5|list [port]")
    if argv[1] == "list":
        if len(argv) != 2:
            raise SystemExit("usage: detect_esp.py esp32s3|esp32c5|list [port]")
        for port, name in scan(ports, detect_chip):
            print(f"{port} {name}")
        return
    want = chip_name(argv[1])
    if want is None:
        raise SystemExit(f"unknown chip {argv[1]}")
    if len(argv) == 3:
        ports = [argv[2]]
    print(choose(want, scan(ports, detect_chip)))


def _esptool_detect():
    import esptool

    return esptool.detect_chip


if __name__ == "__main__":
    main(sys.argv, _esptool_detect(), serial_ports())
