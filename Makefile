.PHONY: test lint fmt coverage firmware-s3 firmware-c5

CARGO ?= cargo +stable
ESP32_WITH_ENV ?= $(HOME)/.cursor/skills/esp32-dev/scripts/with-env.sh

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
	cd firmware && MCU=esp32s3 IDF_MAINTAINER=1 $(ESP32_WITH_ENV) cargo espflash flash --release --target xtensa-esp32s3-espidf --port /dev/ttyACM0 --partition-table partitions.csv --flash-size 8mb
