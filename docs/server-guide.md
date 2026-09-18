# playr-server

`playr-server` is playr for a machine without a screen, such as a Raspberry Pi with a DAC. It plays on that machine's own output device. A web page controls it from a phone, a tablet or a computer on the same network, and OSC controls it from apps such as TouchOSC.

The page has the window's library, selection and playlists views, key bindings, `:` command line, dialogs, marks and themes. It has no sampler, and it cannot quit the server or name a path. [docs/dev/server.md](https://github.com/shakfu/playr/blob/main/docs/dev/server.md) records the design.

## Install

- **Release archives:** every archive holds `playr-server`. Put it on your `PATH`, for a service `~/.local/bin`.

- **From a clone:** `make install` copies it to `~/.local/bin` with `playr` and `playr-gui`.

- **With cargo:** `cargo install --git https://github.com/shakfu/playr playr-server`.

Only one of `playr`, `playr-gui` and `playr-server` runs at a time. They share the library, `settings.toml` and a lock.

## Run it

```sh
playr scan ~/Music                   # once, to build the library
playr-server                         # prints the address to open
```

It prints `playr-server: http://127.0.0.1:8080/?token=...`. Open that address on the same machine. It stops on Ctrl-C.

The page's Menu has Rescan library, which re-scans the directories `playr scan` recorded. The page cannot name a directory of its own, so it re-scans those and nothing else. It appears only once the library has a recorded directory; `playr scan DIR` adds one.

Run `playr scan` again yourself after adding music, or use that button. Stop the server first: `playr`, `playr-gui` and `playr-server` share one lock, so only one runs at a time.

## From other devices

```sh
playr-server --listen 0.0.0.0:8080
```

Open `http://<name>.local:8080/` from a device on the same network, where `<name>` is the machine's host name, or use its IP address.

### The token

By default every request needs a token. Opening the address with `?token=` sets a cookie for a year, so each browser needs it once. The token is kept in `~/.local/share/playr/server.token`, or `$XDG_DATA_HOME/playr/server.token`, and stays the same across restarts.

To get the address from another computer, over SSH, as the user that runs `playr-server`:

```sh
ssh pi.local 'echo "http://$(hostname).local:8080/?token=$(cat ~/.local/share/playr/server.token)"'
```

To open it on a phone, show it as a QR code and scan it with the camera. `qrencode` runs on the computer: `brew install qrencode`, or `apt install qrencode`.

```sh
ssh pi.local 'echo "http://$(hostname).local:8080/?token=$(cat ~/.local/share/playr/server.token)"' | qrencode -t ansiutf8
```

Delete `server.token` and restart to make a new token. Browsers holding the old one are then refused.

### No token: --open

```sh
playr-server --listen 0.0.0.0:8080 --open
```

`--open` serves the page without a token, for a network where every device is trusted. Anyone who can reach the address controls playr. Two checks still apply:

- The `Host` header must be an IP address, `localhost`, a `.local` name, or a name given with `--host`, so a website whose domain resolves to the machine is refused.

- A POST from a page on another origin is refused, so a website open in your browser cannot send commands.

### Behind a proxy

A reverse proxy in front can add TLS and authentication. Pass the name it serves with `--host music.example.com`. Origins with `https` are accepted.

## The page

- **Desktop:** click a row to move the cursor, double-click to play, right-click for its menu. Your key bindings work, `:` opens the command line with Tab completion, and `?` lists the keys. Shift-click on the progress bar adds a mark there.

- **Tablet:** the same, without the album column.

- **Phone:** tap a row to move the cursor, tap it again to play. `...` opens a row's menu, `-` or `+` selects a track, and More shows volume, speed and mode.

## OSC and TouchOSC

```sh
playr-server --listen 0.0.0.0:8080 --osc 0.0.0.0:9000 --osc-reply 192.168.1.30:9001
```

- `--osc` receives OSC on that address. It controls playback and plays playlists by index, oldest first. OSC has no authentication.

- `--osc-reply` sends title, position, level and the rest to that address, such as the tablet's.

- `playr-server osc-schema` prints every address as JSON.

A TouchOSC layout for these addresses is attached to each release as `playr-<version>-touchosc.tosc`, and `make touchosc` builds it from a clone. In TouchOSC, add an OSC connection over UDP to the machine's address, sending to port 9000 and receiving on port 9001.

## Raspberry Pi

### Before installing

- The Linux arm64 archive needs glibc 2.35 or later. `ldd --version` shows the Pi's.

- playr opens the default output device. `aplay -l` lists the cards. To make card 1, a USB DAC or a DAC HAT, the default, put this in `~/.asoundrc`:

  ```
  defaults.pcm.card 1
  defaults.ctl.card 1
  ```

### As a service

`playr-server.service` is in the Linux archives and in `packaging/linux/`. It runs as a systemd user service, as the user who owns the library:

```sh
install -Dm 644 playr-server.service ~/.config/systemd/user/playr-server.service
systemctl --user daemon-reload
systemctl --user enable --now playr-server
loginctl enable-linger "$USER"          # start at boot, without logging in
```

From a clone, `make install install-service` does the first two lines.

The unit runs `~/.local/bin/playr-server --listen 0.0.0.0:8080`.

With music on a separate disk, uncomment `RequiresMountsFor=` in the unit and name the mount point. Without it the server can start before the disk is mounted and re-scan an empty directory.

| to | run |
|-|-|
| change the flags, such as adding `--open` | `systemctl --user edit --full playr-server` |
| see its output and the address | `journalctl --user -u playr-server` |
| stop or start it | `systemctl --user stop playr-server`, `systemctl --user start playr-server` |

### Using the terminal over SSH

`playr`, `playr scan` and `playr prune` refuse to start while the server runs. Log in as the service's user, then:

```sh
systemctl --user stop playr-server
playr                                   # or playr scan, playr prune
systemctl --user start playr-server
```

## Flags

| flag | default | does |
|-|-|-|
| `--listen ADDR:PORT` | `127.0.0.1:8080` | where the page is served |
| `--open` | off | serve the page without a token |
| `--host NAME` | none | a name the page may be opened by; repeatable |
| `--osc ADDR:PORT` | off | receive OSC |
| `--osc-reply ADDR:PORT` | off | send the state as OSC |
| `--db PATH` | `~/.local/share/playr/library.db` | the library |
| `--settings PATH` | `~/.config/playr/settings.toml` | the settings |
