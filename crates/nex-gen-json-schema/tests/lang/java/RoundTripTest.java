// Round-trips the generated Java models through a stock Jackson ObjectMapper —
// the same object the Temporal Java SDK's default data converter constructs.
// Run by the `lang_roundtrip` Rust harness, which generates the model classes
// into the same package first.
package com.example.chat;

import static org.junit.jupiter.api.Assertions.*;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;

public class RoundTripTest {
  static final ObjectMapper M = new ObjectMapper();

  @Test
  void messageConstAndDefault() throws Exception {
    Message m = new Message("hi", null, null);
    JsonNode w = M.readTree(M.writeValueAsString(m));
    assertEquals("text", w.get("kind").asText());
    assertFalse(w.has("priority"), "default off-the-wire");
    assertFalse(w.has("replyToId"), "optional omitted");
    assertEquals(0L, m.getPriority(), "default surfaced on read");
  }

  @Test
  void messageFullRoundTrip() throws Exception {
    Message m = M.readValue("{\"kind\":\"text\",\"body\":\"yo\",\"replyToId\":null,\"priority\":7}", Message.class);
    assertEquals(7L, m.getPriority());
    JsonNode w = M.readTree(M.writeValueAsString(m));
    assertEquals(7, w.get("priority").asInt());
  }

  @Test
  void roomRequiredNullableAndOpen() throws Exception {
    String in = "{\"roomId\":\"r1\",\"displayName\":\"General\",\"topic\":null,\"members\":[\"a\"],\"x-extra\":42}";
    Room r = M.readValue(in, Room.class);
    assertNull(r.getTopic());
    JsonNode w = M.readTree(M.writeValueAsString(r));
    assertTrue(w.has("topic") && w.get("topic").isNull(), "required+nullable emits null");
    assertEquals(42, w.get("x-extra").asInt(), "open struct preserves extras");
  }

  @Test
  void labelsTypedMap() throws Exception {
    Labels l = M.readValue("{\"env\":\"prod\",\"team\":\"core\"}", Labels.class);
    JsonNode w = M.readTree(M.writeValueAsString(l));
    assertEquals("prod", w.get("env").asText());
    assertEquals("core", w.get("team").asText());
  }

  @Test
  void closedStructRejectsUnknown() {
    assertThrows(
        Exception.class,
        () ->
            M.readValue(
                "{\"roomId\":\"r\",\"message\":{\"kind\":\"text\",\"body\":\"x\"},\"nope\":1}",
                SendMessageInput.class));
  }

  @Test
  void constViolation() {
    assertThrows(Exception.class, () -> M.readValue("{\"kind\":\"other\",\"body\":\"x\"}", Message.class));
  }

  @Test
  void missingRequired() {
    assertThrows(Exception.class, () -> M.readValue("{}", SendMessageOutput.class));
  }
}
