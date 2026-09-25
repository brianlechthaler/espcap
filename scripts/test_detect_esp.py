import io
import runpy
import sys
import types
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest.mock import patch

import detect_esp
from detect_esp import chip_name, choose, identify, main, scan, serial_ports


class ChooseTest(unittest.TestCase):
    def test_chip_name(self):
        self.assertEqual(chip_name("esp32s3"), "ESP32-S3")
        self.assertEqual(chip_name("esp32c5"), "ESP32-C5")
        self.assertIsNone(chip_name("esp32"))

    def test_one_match(self):
        found = [("/dev/ttyACM0", "ESP32-S3"), ("/dev/ttyACM1", "ESP32-C5")]
        self.assertEqual(choose("ESP32-C5", found), "/dev/ttyACM1")

    def test_none(self):
        with self.assertRaises(SystemExit) as ctx:
            choose("ESP32-C5", [("/dev/ttyACM0", "ESP32-S3")])
        self.assertIn("no ESP32-C5", str(ctx.exception))
        self.assertIn("/dev/ttyACM0=ESP32-S3", str(ctx.exception))

    def test_none_attached(self):
        with self.assertRaises(SystemExit) as ctx:
            choose("ESP32-S3", [])
        self.assertIn("none", str(ctx.exception))

    def test_multiple(self):
        found = [("/dev/ttyACM0", "ESP32-C5"), ("/dev/ttyACM2", "ESP32-C5")]
        with self.assertRaises(SystemExit) as ctx:
            choose("ESP32-C5", found)
        self.assertIn("/dev/ttyACM0", str(ctx.exception))
        self.assertIn("/dev/ttyACM2", str(ctx.exception))


class FakeChip:
    CHIP_NAME = "ESP32-C5"

    def __init__(self, reset_error=False, close_error=False):
        self.reset_error = reset_error
        self.close_error = close_error
        self._port = self

    def hard_reset(self):
        if self.reset_error:
            raise RuntimeError("reset")

    def close(self):
        if self.close_error:
            raise RuntimeError("close")


class ScanTest(unittest.TestCase):
    def test_serial_ports_sorted_unique(self):
        def globber(pattern):
            if pattern.endswith("ACM*"):
                return ["/dev/ttyACM1", "/dev/ttyACM0"]
            return ["/dev/ttyACM0", "/dev/ttyUSB0"]

        self.assertEqual(
            serial_ports(globber),
            ["/dev/ttyACM0", "/dev/ttyACM1", "/dev/ttyUSB0"],
        )

    def test_identify_name_and_failures(self):
        def noisy(*a, **k):
            print("Connecting...")
            return FakeChip()

        buf = io.StringIO()
        err = io.StringIO()
        with redirect_stdout(buf), redirect_stderr(err):
            self.assertEqual(identify("/dev/ttyACM0", noisy), "ESP32-C5")
        self.assertEqual(buf.getvalue(), "")
        self.assertIn("Connecting...", err.getvalue())
        chip = FakeChip()
        self.assertEqual(identify("/dev/ttyACM0", lambda *a, **k: chip), "ESP32-C5")
        self.assertIsNone(
            identify("/dev/ttyACM0", lambda *a, **k: (_ for _ in ()).throw(OSError("x")))
        )
        self.assertEqual(
            identify("/dev/ttyACM0", lambda *a, **k: FakeChip(reset_error=True)),
            "ESP32-C5",
        )
        self.assertEqual(
            identify("/dev/ttyACM0", lambda *a, **k: FakeChip(close_error=True)),
            "ESP32-C5",
        )

    def test_scan_skips_unknown(self):
        def detect(port, connect_attempts=1):
            if port.endswith("0"):
                raise OSError("nope")
            return FakeChip()

        self.assertEqual(
            scan(["/dev/ttyACM0", "/dev/ttyACM1"], detect),
            [("/dev/ttyACM1", "ESP32-C5")],
        )

    def test_main_list_select_and_errors(self):
        def detect(port, connect_attempts=1):
            return FakeChip()

        buf = io.StringIO()
        with redirect_stdout(buf):
            main(["detect_esp.py", "list"], detect, ["/dev/ttyACM1"])
        self.assertEqual(buf.getvalue(), "/dev/ttyACM1 ESP32-C5\n")

        buf = io.StringIO()
        with redirect_stdout(buf):
            main(["detect_esp.py", "esp32c5"], detect, ["/dev/ttyACM1"])
        self.assertEqual(buf.getvalue(), "/dev/ttyACM1\n")

        with self.assertRaises(SystemExit) as ctx:
            main(["detect_esp.py"], detect, [])
        self.assertIn("usage", str(ctx.exception))
        with self.assertRaises(SystemExit):
            main(["detect_esp.py", "--help"], detect, [])
        with self.assertRaises(SystemExit) as ctx:
            main(["detect_esp.py", "esp32"], detect, [])
        self.assertIn("unknown chip", str(ctx.exception))
        with self.assertRaises(SystemExit) as ctx:
            main(["detect_esp.py", "list", "/dev/ttyACM0"], detect, [])
        self.assertIn("usage", str(ctx.exception))

        with self.assertRaises(SystemExit) as ctx:
            main(
                ["detect_esp.py", "esp32c5", "/dev/ttyACM0"],
                lambda *a, **k: (_ for _ in ()).throw(AssertionError("scan all")),
                ["/dev/ttyACM1"],
            )
        self.assertIn("no ESP32-C5", str(ctx.exception))

        s3 = FakeChip()
        s3.CHIP_NAME = "ESP32-S3"
        with self.assertRaises(SystemExit) as ctx:
            main(["detect_esp.py", "esp32c5", "/dev/ttyACM0"], lambda *a, **k: s3, [])
        self.assertIn("/dev/ttyACM0=ESP32-S3", str(ctx.exception))

        buf = io.StringIO()
        with redirect_stdout(buf):
            main(["detect_esp.py", "esp32c5", "/dev/ttyACM1"], detect, [])
        self.assertEqual(buf.getvalue(), "/dev/ttyACM1\n")

    def test_esptool_detect_uses_module(self):
        module = types.ModuleType("esptool")
        module.detect_chip = object()
        with patch.dict(sys.modules, {"esptool": module}):
            self.assertIs(detect_esp._esptool_detect(), module.detect_chip)

    def test_script_entry_lists_nothing_when_no_ports(self):
        import glob

        esptool = types.ModuleType("esptool")
        esptool.detect_chip = lambda *a, **k: None
        argv = ["detect_esp.py", "list"]
        with (
            patch.object(glob, "glob", return_value=[]),
            patch.dict(sys.modules, {"esptool": esptool}),
            patch.object(sys, "argv", argv),
            redirect_stdout(io.StringIO()) as buf,
        ):
            runpy.run_path(detect_esp.__file__, run_name="__main__")
        self.assertEqual(buf.getvalue(), "")

    def test_entry_with_no_ports(self):
        with (
            patch.object(sys, "argv", ["detect_esp.py", "list"]),
            patch.object(detect_esp, "serial_ports", return_value=[]),
            patch.object(detect_esp, "_esptool_detect", return_value=lambda *a, **k: None),
            redirect_stdout(io.StringIO()) as buf,
        ):
            detect_esp.main(
                sys.argv, detect_esp._esptool_detect(), detect_esp.serial_ports()
            )
        self.assertEqual(buf.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
