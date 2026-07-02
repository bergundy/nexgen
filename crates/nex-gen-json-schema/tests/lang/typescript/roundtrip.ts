// Round-trips the generated TypeScript models through the generated
// parse/serialize helpers. The Temporal TS SDK has no type hints, so a payload
// converter uses these generated functions to (de)serialize a typed value —
// this exercises that helper path. Run by the `lang_roundtrip` Rust harness,
// which generates index.ts alongside this file.
import {
  parseMessage,
  serializeMessage,
  parseRoom,
  serializeRoom,
  parseLabels,
  serializeLabels,
  parseSendMessageInput,
  parseSendMessageOutput,
  DEFAULT_PRIORITY,
} from "./index";

function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error("FAIL: " + msg);
}

// Message: const kind, off-the-wire default, optional replyToId.
const m = parseMessage({ kind: "text", body: "hi" });
const mw = serializeMessage(m) as Record<string, unknown>;
assert(mw.kind === "text", "kind const");
assert(!("priority" in mw), "default off-the-wire");
assert(!("replyToId" in mw), "optional omitted");
assert((m.priority ?? DEFAULT_PRIORITY) === 0, "default on read");

// Full message round-trips faithfully (optional+nullable + set priority).
const m2 = parseMessage({ kind: "text", body: "yo", replyToId: null, priority: 7 });
const m2w = serializeMessage(m2) as Record<string, unknown>;
assert(m2w.priority === 7, "priority round-trips");
assert(m2w.replyToId === null, "optional+nullable null round-trips");

// Room: required+nullable topic -> null; open struct preserves extras.
const r = parseRoom({ roomId: "r1", displayName: "General", topic: null, members: ["a"], "x-extra": 42 });
const rw = serializeRoom(r) as Record<string, unknown>;
assert(rw.topic === null, "required+nullable emits null");
assert(rw["x-extra"] === 42, "open struct preserves extras");

// Labels: typed map.
const l = parseLabels({ env: "prod", team: "core" });
const lw = serializeLabels(l) as Record<string, unknown>;
assert(lw.env === "prod" && lw.team === "core", "typed map round-trips");

// Closed struct rejects unknown key.
let threw = false;
try {
  parseSendMessageInput({ roomId: "r", message: { kind: "text", body: "x" }, nope: 1 });
} catch {
  threw = true;
}
assert(threw, "closed struct rejects unknown");

// const violation rejected.
threw = false;
try {
  parseMessage({ kind: "other", body: "x" });
} catch {
  threw = true;
}
assert(threw, "const violation rejected");

// missing required rejected.
threw = false;
try {
  parseSendMessageOutput({});
} catch {
  threw = true;
}
assert(threw, "missing required rejected");

console.log("TS ROUND-TRIP OK");
