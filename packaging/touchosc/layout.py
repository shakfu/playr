# /// script
# requires-python = ">=3.10"
# dependencies = ["py2tosc>=0.6,<0.7"]
# ///
"""A TouchOSC layout for playr-server, built from its OSC schema.

    playr-server osc-schema > schema.json
    uv run packaging/touchosc/layout.py schema.json playr.tosc

`make touchosc` does both. Every address in the schema needs a control here,
and every control's address must be in the schema, so a layout that has fallen
behind the server is refused rather than written.
"""

from __future__ import annotations

import argparse
import json
import sys

import py2tosc
from py2tosc import Conversion, Orientation, TriggerCondition, ui

#: A tablet in landscape.
FRAME = (0, 0, 1024, 768)

GAP = 8

ACCENT = "#4fc3cf"
LEVEL = "#5cb85c"
INK = "#e8e8e8"


def sends(address: str, *, args=None, on: str = "ANY") -> py2tosc.OscMessage:
    """A binding that sends to `address` and ignores what arrives there."""
    return ui.osc(address, args=args, on=on, receive=False)


def mirrors(address: str, *, args=None) -> py2tosc.OscMessage:
    """A binding that sends to `address` and takes the value the server sends back."""
    return ui.osc(address, args=args)


def shows(address: str, *, args=None) -> py2tosc.OscMessage:
    """A binding that only takes the value the server sends to `address`."""
    return ui.osc(address, args=args, triggers=[], send=False)


def label(name: str, address: str, size: int, placeholder: str) -> py2tosc.Control:
    """A label showing the text the server sends to `address`."""
    control = py2tosc.label(
        name=name,
        text_size=size,
        text_color=INK,
        messages=[shows(address, args=[ui.value("text", conversion=Conversion.STRING)])],
    )
    control.value("text").default = placeholder
    return control


def captioned(control: py2tosc.Control, text: str, size: int) -> py2tosc.Control:
    """`control` with `text` over it, legible on the control's colour."""
    group = ui.labelled(control, text, size=size)
    group[1].text_color = INK
    return group


def fader(name: str, message: py2tosc.OscMessage, **props) -> py2tosc.Control:
    props.setdefault("color", ACCENT)
    return py2tosc.fader(
        name=name, orientation=Orientation.EAST, messages=[message], **props
    )


def radio(name: str, message: py2tosc.OscMessage, words: list[str], **props) -> py2tosc.Control:
    """A radio with a caption over each segment. Its `x` is the segment's index."""
    control = py2tosc.radio(
        name=name,
        steps=len(words),
        orientation=Orientation.EAST,
        color=ACCENT,
        messages=[message],
        **props,
    )
    captions = [
        py2tosc.label(
            name=word, background=False, interactive=False, text_size=20, text_color=INK
        )
        for word in words
    ]
    for caption, word in zip(captions, words):
        caption.value("text").default = word
    return ui.stack(control, ui.row(*captions, name=f"{name} captions"), name=f"{name} group")


def index(i: int) -> list:
    return [ui.const(str(i), conversion=Conversion.FLOAT)]


def build(playlists: int) -> py2tosc.Document:
    integer = [ui.value("x", conversion=Conversion.INTEGER)]
    now = ui.column(
        label("title", "/playr/title", 36, "playr"),
        label("artist", "/playr/artist", 24, ""),
        ui.row(
            label("time", "/playr/time", 20, "0:00 / --"),
            radio(
                "state",
                shows("/playr/state", args=integer),
                ["Stopped", "Playing", "Paused"],
                interactive=False,
            ),
            sizes=(1, 2),
            gap=GAP,
        ),
        sizes=(2, 1, 1),
        name="now playing",
    )
    transport = ui.row(
        [
            captioned(
                py2tosc.button(name=word.lower(), color=ACCENT, messages=[sends(address)]),
                word,
                24,
            )
            for word, address in [
                ("Prev", "/playr/prev"),
                ("Pause", "/playr/pause"),
                ("Stop", "/playr/stop"),
                ("Next", "/playr/next"),
            ]
        ],
        gap=GAP,
        name="transport",
    )
    settings = ui.column(
        captioned(fader("volume", mirrors("/playr/volume")), "Volume", 20),
        captioned(
            fader(
                "speed",
                mirrors("/playr/speed", args=[ui.value("x", scale=(-12.0, 12.0))]),
                centered=True,
            ),
            "Speed",
            20,
        ),
        radio(
            "mode",
            mirrors("/playr/mode", args=integer),
            ["Normal", "Shuffle", "Repeat", "Repeat one"],
        ),
        gap=GAP,
        name="settings",
    )
    lists = ui.tiles(
        [
            captioned(
                py2tosc.button(
                    name=f"playlist {i + 1}",
                    color=ACCENT,
                    messages=[sends("/playr/playlist", args=index(i), on=TriggerCondition.RISE)],
                ),
                str(i + 1),
                24,
            )
            for i in range(playlists)
        ],
        columns=min(playlists, 8),
        gap=GAP,
        name="playlists",
    )
    root = ui.column(
        now,
        fader("progress", mirrors("/playr/progress")),
        fader("level", shows("/playr/level"), interactive=False, color=LEVEL),
        transport,
        settings,
        lists,
        sizes=(3, 1, 0.4, 1.5, 3, 1.5),
        gap=GAP,
        pad=GAP,
        frame=FRAME,
        name="playr",
    )
    doc = py2tosc.Document(root=root)
    doc.resolve()
    return doc


def addresses(doc: py2tosc.Document) -> tuple[set[str], set[str]]:
    """The addresses the layout sends to, and those it takes values from."""
    sent, taken = set(), set()
    for control in doc.walk():
        for message in control.messages:
            if not isinstance(message, py2tosc.OscMessage):
                continue
            address = "".join(p.value for p in message.path)
            if message.send:
                sent.add(address)
            if message.receive:
                taken.add(address)
    return sent, taken


def mismatches(schema: dict, doc: py2tosc.Document) -> list[str]:
    """Where the layout and the schema disagree, in words."""
    sent, taken = addresses(doc)
    received = {a["address"] for a in schema["received"]}
    replies = {a["address"] for a in schema["sent"]}
    problems = []
    for address in sorted(received - sent):
        problems.append(f"the server receives {address}, but no control sends it")
    for address in sorted(sent - received):
        problems.append(f"a control sends {address}, which the server does not receive")
    for address in sorted(replies - taken):
        problems.append(f"the server sends {address}, but no control shows it")
    for address in sorted(taken - replies):
        problems.append(f"a control waits for {address}, which the server does not send")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("schema", help="the output of playr-server osc-schema")
    parser.add_argument("output", help="the layout to write: .tosc, .xml or .json")
    parser.add_argument("--playlists", type=int, default=8, help="playlist buttons, 1 to 32")
    args = parser.parse_args()
    if not 1 <= args.playlists <= 32:
        parser.error("--playlists is 1 to 32")

    with open(args.schema, encoding="utf-8") as f:
        schema = json.load(f)
    doc = build(args.playlists)
    problems = mismatches(schema, doc)
    if problems:
        for problem in problems:
            print(f"layout.py: {problem}", file=sys.stderr)
        return 1
    # Refuses to write a layout py2tosc finds errors in.
    doc.save(args.output, validate=True)
    for issue in doc.validate():
        print(f"layout.py: {issue}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
