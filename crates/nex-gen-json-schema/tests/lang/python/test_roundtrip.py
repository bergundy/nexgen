# Round-trips the generated Python models through Temporal's Pydantic data
# converter (type hints). Run by the `lang_roundtrip` Rust harness, which
# generates `chat/__init__.py` into the same directory first.
import json

from temporalio.contrib.pydantic import pydantic_data_converter

import chat

conv = pydantic_data_converter.payload_converter


def roundtrip(value, typ):
    payloads = conv.to_payloads([value])
    back = conv.from_payloads(payloads, [typ])[0]
    return json.loads(payloads[0].data), back


# Message: const kind auto-emitted, default priority off-the-wire,
# optional+nullable replyToId omitted when absent.
m = chat.Message(body="hi")
wire, back = roundtrip(m, chat.Message)
assert wire["kind"] == "text", wire
assert "priority" not in wire, "default must be off-the-wire"
assert "replyToId" not in wire, wire
assert back.priority == 0, "default surfaced on read"

# Round-trips a fully-populated message faithfully.
m2 = chat.Message.model_validate({"kind": "text", "body": "yo", "replyToId": None, "priority": 7})
wire, back = roundtrip(m2, chat.Message)
assert wire["priority"] == 7, wire
assert wire["replyToId"] is None, "optional+nullable null round-trips"
assert back.priority == 7

# Room: required+nullable topic emits null; open struct preserves extras.
r = chat.Room.model_validate(
    {"roomId": "r1", "displayName": "General", "topic": None, "members": ["a"], "x-extra": 42}
)
wire, back = roundtrip(r, chat.Room)
assert wire["topic"] is None, "required+nullable must emit null"
assert wire.get("x-extra") == 42, "open struct must preserve extras"
assert back.topic is None

# Labels: typed map + maxProperties.
labels = chat.Labels.model_validate({"env": "prod", "team": "core"})
wire, _ = roundtrip(labels, chat.Labels)
assert wire == {"env": "prod", "team": "core"}, wire

# Closed struct rejects an unknown key.
try:
    chat.SendMessageInput.model_validate(
        {"roomId": "r", "message": {"kind": "text", "body": "x"}, "nope": 1}
    )
    raise SystemExit("closed struct accepted unknown field")
except Exception:
    pass

# const violation is rejected.
try:
    chat.Message.model_validate({"kind": "other", "body": "x"})
    raise SystemExit("const violation accepted")
except Exception:
    pass

# Required member absent is rejected.
try:
    chat.SendMessageOutput.model_validate({})
    raise SystemExit("missing required accepted")
except Exception:
    pass

print("PYTHON ROUND-TRIP OK")
