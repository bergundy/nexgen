// Probe: MATERIALIZE model (B) in .NET via DateTimeOffset / DateOnly / TimeOnly.
// PROSPECTIVE target. Parse each validated wire string, re-serialize to the
// CANONICAL form, emit the bytes. dotnet run -- ../corpus.json
using System.Globalization;
using System.Text.Json;
using System.Text.RegularExpressions;

const string ENGINE = "dotnet";

void Emit(string id, string fmt, string canonical, string err) {
    var o = new { id, engine = ENGINE, format = fmt, canonical, err };
    Console.WriteLine(JsonSerializer.Serialize(o));
}

// date-time -> UTC, ms, "YYYY-MM-DDTHH:MM:SS.mmmZ".
// DateTimeOffset.Parse rejects :60, accepts missing offset (not exercised here).
string CanonDateTime(string wire) {
    var dto = DateTimeOffset.Parse(wire, CultureInfo.InvariantCulture,
        DateTimeStyles.AssumeUniversal | DateTimeStyles.RoundtripKind);
    var u = dto.ToUniversalTime();
    // truncate to ms
    long ms = u.Millisecond;
    return $"{u.Year:D4}-{u.Month:D2}-{u.Day:D2}T{u.Hour:D2}:{u.Minute:D2}:{u.Second:D2}.{ms:D3}Z";
}

string CanonDate(string wire) {
    var d = DateOnly.ParseExact(wire, "yyyy-MM-dd", CultureInfo.InvariantCulture);
    return $"{d.Year:D4}-{d.Month:D2}-{d.Day:D2}";
}

// time -> TimeOnly cannot hold an offset; strip it (wall clock).
string CanonTime(string wire) {
    var s = Regex.Replace(wire, "(Z|[+-][0-9]{2}:[0-9]{2})$", "");
    var t = TimeOnly.Parse(s, CultureInfo.InvariantCulture);
    return $"{t.Hour:D2}:{t.Minute:D2}:{t.Second:D2}.{t.Millisecond:D3}";
}

void Run(JsonElement arr, string fmt, Func<string, string> fn) {
    foreach (var r in arr.EnumerateArray()) {
        var id = r.GetProperty("id").GetString()!;
        var wire = r.GetProperty("wire").GetString()!;
        try { Emit(id, fmt, fn(wire), ""); }
        catch (Exception e) { Emit(id, fmt, "", e.GetType().Name + ": " + e.Message); }
    }
}

var path = args[0];
using var doc = JsonDocument.Parse(File.ReadAllText(path));
var root = doc.RootElement;
Run(root.GetProperty("date-time"), "date-time", CanonDateTime);
Run(root.GetProperty("date"), "date", CanonDate);
Run(root.GetProperty("time"), "time", CanonTime);
