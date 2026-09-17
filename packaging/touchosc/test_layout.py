"""The TouchOSC layout against playr-server's OSC schema.

    make touchosc-test
"""

from __future__ import annotations

import copy
import json
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))
import layout  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]


@pytest.fixture(scope="module")
def schema() -> dict:
    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "playr-server", "--", "osc-schema"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def test_the_layout_matches_the_server(schema):
    assert layout.mismatches(schema, layout.build(8)) == []


def test_an_address_the_layout_lacks_is_reported(schema):
    grown = copy.deepcopy(schema)
    grown["received"].append({"address": "/playr/shuffle", "type": "trigger"})
    grown["sent"] = [a for a in grown["sent"] if a["address"] != "/playr/level"]
    assert layout.mismatches(grown, layout.build(8)) == [
        "the server receives /playr/shuffle, but no control sends it",
        "a control waits for /playr/level, which the server does not send",
    ]


def test_playlist_buttons_send_their_index_on_press():
    doc = layout.build(3)
    for i in range(3):
        (message,) = doc.find(f"playlist {i + 1}").messages
        assert "".join(p.value for p in message.path) == "/playr/playlist"
        assert [p.value for p in message.arguments] == [str(i)]
        assert [(t.var, str(t.condition)) for t in message.triggers] == [("x", "RISE")]
        assert (message.send, message.receive) == (True, False)


def test_py2tosc_finds_nothing_wrong():
    issues = layout.build(8).validate()
    assert [str(i) for i in issues] == []


def test_the_layout_round_trips(tmp_path):
    path = tmp_path / "playr.tosc"
    layout.build(8).save(str(path), validate=True)
    loaded = layout.py2tosc.load(str(path))
    assert loaded.find("progress") is not None
