"""The web page in a real browser, against a real playr-server.

    make page-test

The server runs on a library of generated WAV files, at volume 0, with its
data and settings in a temporary directory. It opens the default audio device.
"""

from __future__ import annotations

import os
import re
import socket
import struct
import subprocess
import time
import wave
from pathlib import Path

import pytest
from playwright.sync_api import Page, expect, sync_playwright

ROOT = Path(__file__).resolve().parents[4]
ALBUMS = {"Alpha": ["One", "Two", "Three"], "Beta": ["Four", "Five"]}


def write_wav(path: Path, seconds: float) -> None:
    rate = 44_100
    with wave.open(str(path), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(struct.pack("<hh", 0, 0) * int(rate * seconds))


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture(scope="module")
def server(tmp_path_factory):
    home = tmp_path_factory.mktemp("playr")
    music = home / "music"
    for album, titles in ALBUMS.items():
        (music / album).mkdir(parents=True)
        for n, title in enumerate(titles, 1):
            write_wav(music / album / f"{n:02} {title}.wav", 30.0)
    (home / "config" / "playr").mkdir(parents=True)
    (home / "config" / "playr" / "settings.toml").write_text(
        f'volume = 0\nsamples = "{home / "samples"}"\n'
    )
    env = dict(os.environ, XDG_DATA_HOME=str(home / "data"), XDG_CONFIG_HOME=str(home / "config"))
    subprocess.run(["cargo", "build", "-q", "-p", "playr", "-p", "playr-server"], cwd=ROOT, check=True)
    target = ROOT / "target" / "debug"
    subprocess.run([target / "playr", "scan", str(music)], env=env, check=True, capture_output=True)
    port = free_port()
    proc = subprocess.Popen(
        [target / "playr-server", "--listen", f"127.0.0.1:{port}"],
        env=env, stdout=subprocess.PIPE, text=True,
    )
    url = re.search(r"http://\S+", proc.stdout.readline()).group(0)
    yield url
    proc.terminate()
    proc.wait(timeout=5)


@pytest.fixture(scope="module")
def browser():
    with sync_playwright() as p:
        b = p.chromium.launch()
        yield b
        b.close()


@pytest.fixture
def open_page(browser, server):
    """Opens the page with `options`, returning it and its errors. Pages are
    closed after the test: each holds an event stream, and Chromium opens six
    connections to a host at most."""
    pages = []

    def opener(**options) -> tuple[Page, list[str]]:
        page = browser.new_page(**options)
        pages.append(page)
        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.on("console", lambda m: m.type == "error" and errors.append(m.text))
        page.goto(server)
        page.wait_for_function("state !== null")
        # Tests share one server, so each starts from the whole library.
        page.evaluate("call('/search', JSON.stringify({ query: '', done: true }))")
        page.evaluate("call('/command', 'view library')")
        until(page, lambda s: s["view"] == "library" and not s["searching"], "the library view")
        page.wait_for_selector(".row:not(.loading)")
        return page, errors

    yield opener
    for page in pages:
        page.close()


def state(page: Page) -> dict:
    return page.evaluate("state")


def until(page: Page, test, what: str, timeout: float = 4.0) -> dict:
    """The state once `test` holds for it."""
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        s = state(page)
        if s and test(s):
            return s
        page.wait_for_timeout(50)
    pytest.fail(f"timed out waiting for {what}: {state(page)}")


def test_keys_move_the_cursor_and_refused_keys_do_nothing(open_page):
    page, errors = open_page(viewport={"width": 1280, "height": 800})
    page.focus("#list")
    page.keyboard.press("j")
    page.keyboard.press("j")
    until(page, lambda s: s["cursors"]["library"] == 2, "two rows down")
    page.keyboard.press("4")
    page.keyboard.press("q")
    page.wait_for_timeout(300)
    assert state(page)["view"] == "library"
    assert errors == []


def test_a_double_click_plays_and_the_bar_seeks_and_marks(open_page):
    page, errors = open_page(viewport={"width": 1280, "height": 800})
    page.locator(".row", has_text="Two").dblclick()
    until(page, lambda s: s["state"] == "playing" and s["title"] == "02 Two", "playing Two")
    bar = page.locator("#bar").bounding_box()
    page.mouse.click(bar["x"] + bar["width"] * 0.5, bar["y"] + 5)
    until(page, lambda s: s["position"] >= 14, "the seek")
    page.keyboard.down("Shift")
    page.mouse.click(bar["x"] + bar["width"] * 0.8, bar["y"] + 5)
    page.keyboard.up("Shift")
    until(page, lambda s: len(s["marks"]) == 1, "a mark")
    expect(page.locator("#ticks .tick")).to_have_count(1)
    page.focus("#list")
    page.keyboard.press("C")
    until(page, lambda s: s["input"]["kind"] == "confirm", "the question")
    page.keyboard.press("y")
    until(page, lambda s: s["marks"] == [], "marks cleared")
    page.click("[data-command=stop]")
    until(page, lambda s: s["state"] == "stopped", "stop")
    assert errors == []


def test_a_selection_is_saved_and_the_playlist_deleted(open_page):
    page, errors = open_page(viewport={"width": 1280, "height": 800})
    for title in ["Four", "One"]:
        page.locator(".row", has_text=title).locator("[data-row-command]").click()
    until(page, lambda s: s["counts"]["selection"] == 2, "two selected")
    page.click(".tab[data-view=selection]")
    page.click("#selection-bar [data-command=save]")
    until(page, lambda s: s["input"]["kind"] == "save", "the save prompt")
    page.fill("#name", "web test")
    page.click("#name-ok")
    until(page, lambda s: s["counts"]["playlists"] == 1, "saved")
    page.click(".tab[data-view=playlists]")
    page.locator(".row.playlist", has_text="web test").click(button="right")
    page.locator("#sheet-items button", has_text="Delete").click()
    s = until(page, lambda s: s["input"]["kind"] == "confirm", "the question")
    assert s["input"]["question"] == 'delete playlist "web test"?'
    page.click("#yes")
    until(page, lambda s: s["counts"]["playlists"] == 0, "deleted")
    page.click(".tab[data-view=selection]")
    page.click("#selection-bar [data-command=clear]")
    until(page, lambda s: s["input"]["kind"] == "confirm", "clear question")
    page.click("#yes")
    until(page, lambda s: s["counts"]["selection"] == 0, "cleared")
    assert errors == []


def test_search_and_the_command_line(open_page):
    page, errors = open_page(viewport={"width": 1280, "height": 800})
    page.focus("#list")
    page.keyboard.press("/")
    until(page, lambda s: s["input"]["kind"] == "search", "the search prompt")
    assert page.evaluate("document.activeElement.id") == "search"
    # The files have no tags, so their names are searched.
    page.keyboard.type("four")
    until(page, lambda s: s["searching"] and s["counts"]["library"] == 1, "one match")
    page.keyboard.press("Escape")
    until(page, lambda s: not s["searching"] and s["counts"]["library"] == 5, "cleared")
    page.focus("#list")
    page.keyboard.press(":")
    until(page, lambda s: s["input"]["kind"] == "command", "the command line")
    page.keyboard.type("mode sh")
    page.keyboard.press("Tab")
    expect(page.locator("#command")).to_have_value("mode shuffle")
    page.keyboard.press("Enter")
    until(page, lambda s: s["mode"] == "shuffle", "shuffle")
    page.evaluate("call('/command', 'mode normal')")
    assert errors == []


def test_the_keys_list_has_only_what_the_page_may_do(open_page):
    page, errors = open_page(viewport={"width": 1280, "height": 800})
    page.focus("#list")
    page.keyboard.press("?")
    until(page, lambda s: s["input"]["kind"] == "keys", "the keys list")
    page.wait_for_selector("#list-rows .keys")
    text = page.inner_text("#list-rows")
    assert ":toggle" in text and ":quit" not in text and "sampler" not in text
    page.keyboard.press("Escape")
    until(page, lambda s: s["input"]["kind"] == "none", "closed")
    assert errors == []


@pytest.mark.parametrize("width,height", [(390, 844), (820, 1180), (1280, 800)])
def test_the_page_fits_its_width(open_page, width, height):
    touch = width < 1000
    page, errors = open_page(
        viewport={"width": width, "height": height},
        has_touch=touch, is_mobile=touch,
    )
    page.evaluate("fail('a message long enough that it must be cut off rather than widen the page')")
    assert page.evaluate("document.documentElement.scrollWidth") == width
    assert errors == []


def test_a_phone_taps_to_play_and_opens_row_menus(open_page):
    page, errors = open_page(
        viewport={"width": 390, "height": 844}, has_touch=True, is_mobile=True,
    )
    page.locator(".row", has_text="Five").tap()
    until(page, lambda s: s["cursors"]["library"] is not None and page.locator(".row.cursor", has_text="Five").count() == 1, "the cursor on Five")
    page.locator(".row", has_text="Five").tap()
    until(page, lambda s: s["title"] == "02 Five", "playing Five")
    page.locator(".row", has_text="One").locator("[data-row-menu]").tap()
    page.locator("#sheet-items button", has_text="Select or unselect").tap()
    until(page, lambda s: s["counts"]["selection"] == 1, "selected from the menu")
    assert not page.is_visible("#volume")
    page.tap("#more")
    assert page.is_visible("#volume")
    page.tap("[data-command=stop]")
    page.evaluate("call('/command', 'view selection').then(() => call('/command', 'clear'))")
    until(page, lambda s: s["input"]["kind"] == "confirm", "clear question")
    page.click("#yes")
    until(page, lambda s: s["counts"]["selection"] == 0, "cleared")
    assert errors == []
