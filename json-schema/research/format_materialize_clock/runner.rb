# Probe: MATERIALIZE model (B) in Ruby via stdlib DateTime/Date/Time.
# PROSPECTIVE target. Parse each validated wire string, re-serialize to the
# CANONICAL form, emit the bytes. ruby runner.rb corpus.json
require "json"
require "date"
require "time"

ENGINE = "ruby"

def emit(o)
  puts JSON.generate(o)
end

# date-time -> UTC, ms, "YYYY-MM-DDTHH:MM:SS.mmmZ".
# NOTE: DateTime.rfc3339 CLAMPS leap :60 -> :59 silently (prior finding).
def canon_datetime(wire)
  dt = DateTime.rfc3339(wire.upcase) # raises on missing offset; CLAMPS :60->:59
  u = dt.new_offset(0)
  # truncate to ms
  ms = (u.sec_fraction * 1000).to_i
  format("%04d-%02d-%02dT%02d:%02d:%02d.%03dZ",
         u.year, u.month, u.day, u.hour, u.min, u.sec, ms)
end

def canon_date(wire)
  d = Date.iso8601(wire)
  format("%04d-%02d-%02d", d.year, d.month, d.day)
end

# time -> Ruby has NO time-of-day-only type. Time.parse fabricates today's date.
def canon_time(_wire)
  raise "UNSUPPORTED: Ruby has no time-of-day-only type"
end

def run(rows, fmt, fn)
  rows.each do |r|
    begin
      emit({ id: r["id"], engine: ENGINE, format: fmt, canonical: fn.call(r["wire"]), err: "" })
    rescue => e
      emit({ id: r["id"], engine: ENGINE, format: fmt, canonical: "", err: "#{e.class}: #{e.message}" })
    end
  end
end

c = JSON.parse(File.read(ARGV[0]))
run(c["date-time"], "date-time", method(:canon_datetime))
run(c["date"], "date", method(:canon_date))
run(c["time"], "time", method(:canon_time))
