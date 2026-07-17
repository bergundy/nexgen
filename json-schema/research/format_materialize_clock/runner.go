// Probe: MATERIALIZE model (B). Parse each validated wire string into Go's
// stdlib typed construct, then re-serialize to the CANONICAL form, and emit
// the bytes. The compare harness checks all materializing languages agree.
//
// Canonical serialization under test:
//   date-time -> UTC-normalized, truncated to ms, "YYYY-MM-DDTHH:MM:SS.mmmZ"
//   date      -> "YYYY-MM-DD"
//   time      -> wall clock, offset DROPPED, ms, "HH:MM:SS.mmm"
//
// go run runner.go corpus.json
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"time"
)

type row struct {
	ID   string `json:"id"`
	Wire string `json:"wire"`
}
type corpus struct {
	DateTime []row `json:"date-time"`
	Date     []row `json:"date"`
	Time     []row `json:"time"`
}
type out struct {
	ID       string `json:"id"`
	Engine   string `json:"engine"`
	Format   string `json:"format"`
	Canonical string `json:"canonical"` // canonical bytes, or "" if cannot materialize
	Err      string `json:"err"`
}

const engine = "go"

func emit(o out) {
	b, _ := json.Marshal(o)
	fmt.Println(string(b))
}

func main() {
	data, _ := os.ReadFile(os.Args[1])
	var c corpus
	json.Unmarshal(data, &c)

	// date-time : time.Time (RFC3339Nano). Note: Go REJECTS leap :60 and
	// lowercase t/z, so those rows will error -> proves the narrowing needed.
	for _, r := range c.DateTime {
		// parse-path pre-normalization: uppercase the case-insensitive t/z
		// (pinned grammar accepts lowercase; native parser rejects it). Safe
		// because date-time has no other letters (offset is digits only).
		t, err := time.Parse(time.RFC3339Nano, strings.ToUpper(r.Wire))
		if err != nil {
			emit(out{r.ID, engine, "date-time", "", err.Error()})
			continue
		}
		u := t.UTC().Truncate(time.Millisecond)
		emit(out{r.ID, engine, "date-time", u.Format("2006-01-02T15:04:05.000Z07:00"), ""})
	}

	// date : Go has no date type; reuse time.Time via layout.
	for _, r := range c.Date {
		t, err := time.Parse("2006-01-02", r.Wire)
		if err != nil {
			emit(out{r.ID, engine, "date", "", err.Error()})
			continue
		}
		emit(out{r.ID, engine, "date", t.Format("2006-01-02"), ""})
	}

	// time : Go has no time-of-day type. Parse via a time.Time, drop offset,
	// emit wall clock ms. (Layout accepts optional offset.)
	for _, r := range c.Time {
		var t time.Time
		var err error
		// try with offset then without
		w := strings.ToUpper(r.Wire)
		t, err = time.Parse("15:04:05.999999999Z07:00", w)
		if err != nil {
			t, err = time.Parse("15:04:05.999999999", w)
		}
		if err != nil {
			emit(out{r.ID, engine, "time", "", err.Error()})
			continue
		}
		// wall clock components, ms
		ms := t.Nanosecond() / 1e6
		emit(out{r.ID, engine, "time", fmt.Sprintf("%02d:%02d:%02d.%03d", t.Hour(), t.Minute(), t.Second(), ms), ""})
	}
}
