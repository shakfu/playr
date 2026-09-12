INSTALL_DIR := $(HOME)/.local/bin

.PHONY: all build release test fmt clippy run clean install

all: build

build:
	@cargo build

release:
	@cargo build --release

test:
	@cargo test
	@cargo test --features opus

fmt:
	@cargo fmt

clippy:
	@cargo clippy --all-targets -- -D warnings
	@cargo clippy --all-targets --features opus -- -D warnings

run:
	@cargo run --release

install: release
	@install -d $(INSTALL_DIR)
	@install -m 755 target/release/playr $(INSTALL_DIR)/playr
	@echo "installed executable to $(INSTALL_DIR)"

clean:
	@cargo clean
