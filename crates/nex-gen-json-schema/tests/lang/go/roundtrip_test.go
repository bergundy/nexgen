// Round-trips the generated Go models through encoding/json — the exact path
// the Temporal Go SDK's default payload converter takes. Run by the
// `lang_roundtrip` Rust harness, which generates chat.go alongside this file.
package chat

import (
	"encoding/json"
	"testing"
)

func TestMessageConstAndDefault(t *testing.T) {
	m := Message{Kind: MessageKindText, Body: "hi"}
	b, err := json.Marshal(m)
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]json.RawMessage
	if err := json.Unmarshal(b, &got); err != nil {
		t.Fatal(err)
	}
	if _, ok := got["priority"]; ok {
		t.Fatal("priority default must be off-the-wire")
	}
	if _, ok := got["replyToId"]; ok {
		t.Fatal("optional replyToId must be omitted")
	}
	if string(got["kind"]) != `"text"` {
		t.Fatalf("kind = %s", got["kind"])
	}
	if m.PriorityOrDefault() != 0 {
		t.Fatal("PriorityOrDefault should surface the schema default")
	}
}

func TestMessageFullRoundTrip(t *testing.T) {
	in := `{"kind":"text","body":"yo","replyToId":null,"priority":7}`
	var m Message
	if err := json.Unmarshal([]byte(in), &m); err != nil {
		t.Fatal(err)
	}
	if m.Priority == nil || *m.Priority != 7 {
		t.Fatal("priority should be 7")
	}
	b, _ := json.Marshal(m)
	var got map[string]json.RawMessage
	json.Unmarshal(b, &got)
	if string(got["priority"]) != "7" {
		t.Fatalf("priority round-trip: %s", got["priority"])
	}
}

func TestRoomRequiredNullableAndOpen(t *testing.T) {
	in := `{"roomId":"r1","displayName":"General","topic":null,"members":["a"],"x-extra":42}`
	var r Room
	if err := json.Unmarshal([]byte(in), &r); err != nil {
		t.Fatal(err)
	}
	if r.Topic != nil {
		t.Fatal("topic should be nil (explicit null)")
	}
	b, _ := json.Marshal(r)
	var got map[string]json.RawMessage
	json.Unmarshal(b, &got)
	if string(got["topic"]) != "null" {
		t.Fatalf("required+nullable topic must emit null, got %s", got["topic"])
	}
	if string(got["x-extra"]) != "42" {
		t.Fatal("open struct must preserve extras")
	}
}

func TestClosedStructRejectsUnknown(t *testing.T) {
	var s SendMessageInput
	if err := json.Unmarshal([]byte(`{"roomId":"r","message":{"kind":"text","body":"x"},"nope":1}`), &s); err == nil {
		t.Fatal("closed struct must reject unknown field")
	}
}

func TestConstViolation(t *testing.T) {
	var m Message
	if err := json.Unmarshal([]byte(`{"kind":"other","body":"x"}`), &m); err == nil {
		t.Fatal("const violation must be rejected")
	}
}

func TestMissingRequired(t *testing.T) {
	var s SendMessageOutput
	if err := json.Unmarshal([]byte(`{}`), &s); err == nil {
		t.Fatal("missing required field must be rejected")
	}
}
