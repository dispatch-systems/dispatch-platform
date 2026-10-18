import test from 'node:test';
import { holds } from './support/holds.js';
import { files, isTestFile, ownerOf } from './support/repo.js';
import { lexFile } from './support/rust.js';

// Today's date comes from core's clock, `foundation::clock::today`, never from the system
// clock: a test server pins it, so what a test collects and seeds falls on the same days
// whatever the date. Past core, product code reads no system clock at all, its instants
// coming from `db::now`; core reads it only for an instant, `Utc::now().timestamp_millis()`,
// and today's date only in the clock.
const clock = 'core/foundation/backend/clock.rs';
const systemClock = /\b(?:Utc|Local|SystemTime)\s*::\s*now\s*\(/;
const coreClock =
  /\b(?:Utc|Local)\s*::\s*now\s*\(\s*\)(?!\s*\.\s*timestamp_millis\s*\(\s*\))|\bSystemTime\s*::\s*now\s*\(/;

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
