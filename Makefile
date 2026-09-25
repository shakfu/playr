INSTALL_DIR := $(HOME)/.local/bin
VERSION := $(shell sed -n 's/^version = "\(.*\)"$$/\1/p' Cargo.toml | head -1)
UNAME := $(shell uname -s)
DIAGRAMS := $(patsubst %.d2,%.svg,$(wildcard docs/media/*.d2))

.PHONY: all build release debug test fmt clippy run gui app clean install install-dev diagrams icons touchosc touchosc-test page-test install-service

all: build

build:
	@cargo build

# The terminal, the desktop window and the server, as they ship, with Opus;
# building libopus needs cmake.
release:
	@cargo build --profile dist -p playr -p playr-gui -p playr-server --features playr/opus,playr-gui/opus,playr-server/opus

# The same three, unoptimized, for install-dev.
debug:
	@cargo build -p playr -p playr-gui -p playr-server

test:
	@cargo fmt --check
	@cargo test --workspace
	@cargo test --workspace --features opus

fmt:
	@cargo fmt

clippy:
	@cargo clippy --workspace --all-targets -- -D warnings
	@cargo clippy --workspace --all-targets --features opus -- -D warnings

run:
	@cargo run --release

# The TouchOSC layout for playr-server, built from its OSC addresses; needs uv.
touchosc:
	@cargo run -q -p playr-server -- osc-schema > target/osc-schema.json
	@uv run -q packaging/touchosc/layout.py target/osc-schema.json target/playr.tosc
	@echo "wrote target/playr.tosc"

# Not part of `test`: it needs uv, and py2tosc from PyPI.
touchosc-test:
	@uv run -q --with pytest --with "py2tosc>=0.6,<0.7" pytest -q packaging/touchosc

# playr-server's web page in Chromium. Not part of `test`: it needs uv,
# Playwright's Chromium (`uv run --with playwright==1.62.0 playwright install
# chromium`) and an audio device.
page-test:
	@uv run -q --with pytest --with playwright==1.62.0 pytest -q -p no:cacheprovider crates/playr-server/tests/page

# The desktop window; see docs/dev/gui.md.
gui:
	@cargo run --release -p playr-gui

# The desktop window as target/dist/playr.app; macOS only.
app: release
	@packaging/macos/bundle.sh target/dist target/dist $(VERSION)
	@echo "built target/dist/playr.app"

# The three programs into $(INSTALL_DIR): what ships for install, a debug
# build for install-dev, stripped of its debug info, about 870 MB of it. The desktop window also goes where the system lists
# applications: ~/Applications on macOS, and a desktop entry and icon under
# ~/.local/share on Linux.
install: FROM := target/dist
install: release
install-dev: FROM := target/debug
install-dev: STRIP := -s
install-dev: debug
install install-dev:
	@install -d $(INSTALL_DIR)
	@install $(STRIP) -m 755 $(FROM)/playr $(INSTALL_DIR)/playr
	@install $(STRIP) -m 755 $(FROM)/playr-gui $(INSTALL_DIR)/playr-gui
	@install $(STRIP) -m 755 $(FROM)/playr-server $(INSTALL_DIR)/playr-server
	@echo "installed playr, playr-gui and playr-server from $(FROM) to $(INSTALL_DIR)"
ifeq ($(UNAME),Darwin)
	@install -d $(HOME)/Applications
	@packaging/macos/bundle.sh $(FROM) $(HOME)/Applications $(VERSION)
	@$(if $(STRIP),strip $(HOME)/Applications/playr.app/Contents/MacOS/playr-gui)
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

# Linux: playr-server's systemd user unit. It enables and starts nothing; see
# docs/server-guide.md.
install-service:
	@[ "$(UNAME)" = Linux ] || { echo "install-service is for Linux, with systemd"; exit 1; }
	@install -Dm 644 packaging/linux/playr-server.service $(HOME)/.config/systemd/user/playr-server.service
	@systemctl --user daemon-reload
	@echo "installed playr-server.service; enable it with: systemctl --user enable --now playr-server"

# Renders the desktop window's icons from crates/playr-gui/assets/playr.svg; macOS only.
icons:
	@packaging/icons.sh

# Renders each docs/media/*.d2 whose source is newer than its .svg.
diagrams: $(DIAGRAMS)

docs/media/%.svg: docs/media/%.d2
	@d2 --layout=tala $< $@

clean:
	@cargo clean
