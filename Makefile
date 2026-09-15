INSTALL_DIR := $(HOME)/.local/bin
VERSION := $(shell sed -n 's/^version = "\(.*\)"$$/\1/p' Cargo.toml | head -1)
UNAME := $(shell uname -s)
DIAGRAMS := $(patsubst %.d2,%.svg,$(wildcard docs/media/*.d2))

.PHONY: all build release test fmt clippy run gui app clean install diagrams icons

all: build

build:
	@cargo build

# The terminal and the desktop window.
release:
	@cargo build --release -p playr -p playr-gui

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

# The desktop window; see docs/dev/gui.md.
gui:
	@cargo run --release -p playr-gui

# The desktop window as target/release/playr.app; macOS only.
app: release
	@packaging/macos/bundle.sh target/release target/release $(VERSION)
	@echo "built target/release/playr.app"

# Both programs into $(INSTALL_DIR). The desktop window also goes where the
# system lists applications: ~/Applications on macOS, and a desktop entry and
# icon under ~/.local/share on Linux.
install: release
	@install -d $(INSTALL_DIR)
	@install -m 755 target/release/playr $(INSTALL_DIR)/playr
	@install -m 755 target/release/playr-gui $(INSTALL_DIR)/playr-gui
	@echo "installed playr and playr-gui to $(INSTALL_DIR)"
ifeq ($(UNAME),Darwin)
	@install -d $(HOME)/Applications
	@packaging/macos/bundle.sh target/release $(HOME)/Applications $(VERSION)
	@echo "installed playr.app to $(HOME)/Applications"
else ifeq ($(UNAME),Linux)
	@install -Dm 644 packaging/linux/playr.desktop $(HOME)/.local/share/applications/playr.desktop
	@install -Dm 644 crates/playr-gui/assets/playr.png $(HOME)/.local/share/icons/hicolor/256x256/apps/playr.png
	@# A stale icon-theme.cache hides icons it does not list; rebuild it if present.
	@if [ -f $(HOME)/.local/share/icons/hicolor/icon-theme.cache ]; then \
		gtk-update-icon-cache -qft $(HOME)/.local/share/icons/hicolor || true; fi
	@# GIO reads MimeType= only through mimeinfo.cache.
	@update-desktop-database -q $(HOME)/.local/share/applications || true
	@echo "installed playr.desktop and its icon under $(HOME)/.local/share"
endif

# Renders the desktop window's icons from crates/playr-gui/assets/playr.svg; macOS only.
icons:
	@packaging/icons.sh

# Renders each docs/media/*.d2 whose source is newer than its .svg.
diagrams: $(DIAGRAMS)

docs/media/%.svg: docs/media/%.d2
	@d2 --layout=tala $< $@

clean:
	@cargo clean
