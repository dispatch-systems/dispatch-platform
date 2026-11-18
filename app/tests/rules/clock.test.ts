import test from 'node:test';
import { holds } from './support/holds.js';
import { files, isTestFile, ownerOf, read } from './support/repo.js';
import { lexFile } from './support/rust.js';

// Today's date comes from core's clock, `foundation::clock::today`, never from the system
// clock: a test server pins it, so what a test collects and seeds falls on the same days
// whatever the date. Past core, product code reads no system clock at all, its instants
// coming from `db::now`; core reads it only for an instant, `Timestamp::now().as_millisecond()`,
// and today's date only in the clock.
const clock = 'core/foundation/backend/clock.rs';
// Called or passed as a function: `.unwrap_or_else(Timestamp::now)` reads it too.
const systemClock = /\b(?:Timestamp|ZonedDateTime|SystemTime)\s*::\s*now\b/;
const coreClock =
  /\bTimestamp\s*::\s*now\b(?!\s*\(\s*\)\s*\.\s*as_millisecond\s*\(\s*\))|\b(?:ZonedDateTime|SystemTime)\s*::\s*now\b/;

test("product code reads today's date only from core's clock", () => {
  holds(
    'clock',
    "today's date",
    files
      .filter((file) => file.endsWith('.rs') && !isTestFile(file) && file !== clock)
      .flatMap((file) => {
        const layer = ownerOf(file)?.layer;
        if (!layer) return [];
        return (layer === 'core' ? coreClock : systemClock).test(lexFile(file).masked)
          ? [file]
          : [];
      })
      .map((file) => `${file} reads the system clock: use foundation::clock::today or db::now`),
  );
});

// Dates, times and timezones are jiff's. chrono and chrono-tz are unmaintained (RUSTSEC-2026-0543,
// RUSTSEC-2026-0555), and chrono-tz's timezone rules stopped at 2025b: no crate here takes
// either again.
test('no crate depends on chrono', () => {
  holds(
    'clock',
    'the time library',
    files
      .filter((file) => file.endsWith('Cargo.toml') && /^chrono(?:-tz)?\b/m.test(read(file)))
      .map((file) => `${file} depends on chrono: use jiff, through core's clock`),
  );
});
