// Probe: MATERIALIZE model (B) in Java via java.time (OffsetDateTime /
// LocalDate / LocalTime). Parse each validated wire string into the typed
// construct, re-serialize to the CANONICAL form, emit the bytes.
//   java Runner.java corpus.json
import java.nio.file.*;
import java.time.*;
import java.time.format.*;
import java.util.*;
import java.util.regex.*;

public class Runner {
    static final String ENGINE = "java";

    static void emit(String id, String fmt, String canonical, String err) {
        // minimal JSON escape
        System.out.println("{\"id\":\"" + id + "\",\"engine\":\"" + ENGINE
            + "\",\"format\":\"" + fmt + "\",\"canonical\":\"" + esc(canonical)
            + "\",\"err\":\"" + esc(err) + "\"}");
    }
    static String esc(String s) {
        return s.replace("\\", "\\\\").replace("\"", "\\\"");
    }

    // date-time -> UTC, truncate ms, "YYYY-MM-DDTHH:MM:SS.mmmZ"
    static String canonDateTime(String wire) {
        OffsetDateTime odt = OffsetDateTime.parse(wire); // rejects :60
        Instant i = odt.toInstant();
        OffsetDateTime u = i.atOffset(ZoneOffset.UTC)
            .truncatedTo(java.time.temporal.ChronoUnit.MILLIS);
        int ms = u.getNano() / 1_000_000;
        return String.format("%04d-%02d-%02dT%02d:%02d:%02d.%03dZ",
            u.getYear(), u.getMonthValue(), u.getDayOfMonth(),
            u.getHour(), u.getMinute(), u.getSecond(), ms);
    }

    static String canonDate(String wire) {
        LocalDate d = LocalDate.parse(wire);
        return String.format("%04d-%02d-%02d", d.getYear(), d.getMonthValue(), d.getDayOfMonth());
    }

    // time -> wall clock, offset DROPPED, ms. RFC3339 time has OPTIONAL offset;
    // LocalTime can't hold an offset so we strip it. If an offset is present we
    // parse it out but discard (wall-clock semantics, matching Go/Python).
    static String canonTime(String wire) {
        // strip trailing offset (Z or +/-HH:MM) for LocalTime
        String s = wire;
        Matcher m = Pattern.compile("(Z|[+-][0-9]{2}:[0-9]{2})$").matcher(s);
        if (m.find()) s = s.substring(0, m.start());
        LocalTime t = LocalTime.parse(s);
        int ms = t.getNano() / 1_000_000;
        return String.format("%02d:%02d:%02d.%03d", t.getHour(), t.getMinute(), t.getSecond(), ms);
    }

    public static void main(String[] args) throws Exception {
        String txt = new String(Files.readAllBytes(Paths.get(args[0])));
        // dumb JSON: pull each format array via regex over {"id":...,"wire":...}
        runArray(txt, "date-time", Runner::canonDateTime);
        runArray(txt, "date", Runner::canonDate);
        runArray(txt, "time", Runner::canonTime);
    }

    interface Fn { String apply(String s) throws Exception; }

    static void runArray(String txt, String fmt, Fn fn) {
        // locate the "<fmt>": [ ... ] block
        int key = txt.indexOf("\"" + fmt + "\"");
        int lb = txt.indexOf('[', key);
        int rb = txt.indexOf(']', lb);
        String block = txt.substring(lb, rb);
        Matcher m = Pattern.compile("\\{\\s*\"id\"\\s*:\\s*\"([^\"]*)\"\\s*,\\s*\"wire\"\\s*:\\s*\"([^\"]*)\"").matcher(block);
        while (m.find()) {
            String id = m.group(1), wire = m.group(2);
            try {
                emit(id, fmt, fn.apply(wire), "");
            } catch (Exception e) {
                emit(id, fmt, "", e.getClass().getSimpleName() + ": " + e.getMessage());
            }
        }
    }
}
