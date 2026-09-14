INSTALL_DIR := $(HOME)/.local/bin
DIAGRAMS := $(patsubst %.d2,%.svg,$(wildcard docs/media/*.d2))

.PHONY: all build release test fmt clippy run gui clean install diagrams

all: build

build:
	@cargo build

release:
	@cargo build --release

test:
	@cargo test --workspace
	@cargo test --workspace --features opus

fmt:
	@cargo fmt

clippy:
	@cargo clippy --workspace --all-targets -- -D warnings
	@cargo clippy --workspace --all-targets --features opus -- -D warnings

run:
	@cargo run --release

# The desktop window, still in progress; see docs/dev/gui.md.
gui:
	@cargo run --release -p playr-gui

install: release
	@install -d $(INSTALL_DIR)
	@install -m 755 target/release/playr $(INSTALL_DIR)/playr
	@echo "installed executable to $(INSTALL_DIR)"

# Renders each docs/media/*.d2 whose source is newer than its .svg.
diagrams: $(DIAGRAMS)

docs/media/%.svg: docs/media/%.d2
	@d2 --layout=tala $< $@

clean:
	@cargo clean
