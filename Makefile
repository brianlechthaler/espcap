.PHONY: test lint fmt coverage firmware-s3 firmware-c5 flash-s3 flash-c5 devices

# S3_PORT and C5_PORT override autodetection. Unset means probe USB serial ports.

CARGO ?= cargo +stable
ESP32_DEV_PREFIX ?= $(HOME)/.esp32-dev
ESP32_WITH_ENV ?= $(HOME)/.cursor/skills/esp32-dev/scripts/with-env.sh
ESP32_PYTHON ?= $(ESP32_DEV_PREFIX)/venv/bin/python

test:
	$(CARGO) test --workspace --all-targets

lint:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	$(CARGO) fmt --all

coverage:
	$(CARGO) llvm-cov --workspace --fail-under-functions 100 --fail-under-lines 99 --ignore-filename-regex 'src/main.rs'

firmware-s3:
	cd firmware && MCU=esp32s3 IDF_MAINTAINER=1 $(ESP32_WITH_ENV) cargo build --release --target xtensa-esp32s3-espidf

firmware-c5:
	cd firmware && MCU=esp32c5 IDF_MAINTAINER=1 $(ESP32_WITH_ENV) cargo build --release --target riscv32imac-esp-espidf

flash-s3:
	set -e; \
	port="$(S3_PORT)"; \
	if [ -z "$$port" ]; then port="$$($(ESP32_PYTHON) $(CURDIR)/scripts/detect_esp.py esp32s3)"; fi; \
	$(ESP32_PYTHON) $(CURDIR)/scripts/detect_esp.py esp32s3 "$$port" >/dev/null; \
	cd firmware && MCU=esp32s3 IDF_MAINTAINER=1 $(ESP32_WITH_ENV) cargo espflash flash --release --target xtensa-esp32s3-espidf --port $$port --partition-table partitions.csv --flash-size 8mb

flash-c5:
	set -e; \
	port="$(C5_PORT)"; \
	if [ -z "$$port" ]; then port="$$($(ESP32_PYTHON) $(CURDIR)/scripts/detect_esp.py esp32c5)"; fi; \
	$(ESP32_PYTHON) $(CURDIR)/scripts/detect_esp.py esp32c5 "$$port" >/dev/null; \
	cd firmware && MCU=esp32c5 IDF_MAINTAINER=1 $(ESP32_WITH_ENV) cargo espflash flash --release --target riscv32imac-esp-espidf --port $$port --partition-table partitions.csv --flash-size 8mb

devices:
	$(ESP32_PYTHON) $(CURDIR)/scripts/detect_esp.py list
