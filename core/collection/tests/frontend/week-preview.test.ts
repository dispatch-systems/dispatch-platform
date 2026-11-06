import test from 'node:test';
import assert from 'node:assert/strict';
import { until } from '../../frontend/collectors/WeekPreview.js';

// A run a day or more away counts the days on the calendar where the DSP is, so two runs on
// one day never read as different days away.
test('how long until a run counts the days where the DSP is', () => {
  const timeZone = 'America/Los_Angeles';
  // Thursday, Nov 5 at 11:50 PM in Los Angeles.
  const now = new Date('2026-11-06T07:50:00Z');
  const cases: [string, string][] = [
    ['2026-11-06T19:00:00Z', 'in 11 hr'], // Friday 11:00 AM
    ['2026-11-07T19:00:00Z', 'in 2 days'], // Saturday 11:00 AM, 35 hours away
    ['2026-11-08T00:00:00Z', 'in 2 days'], // Saturday 4:00 PM, 40 hours away
    ['2026-11-08T07:00:00Z', 'in 2 days'], // Saturday 11:00 PM
    ['2026-11-08T08:30:00Z', 'in 3 days'], // Sunday 12:30 AM
  ];
  for (const [run, expected] of cases) assert.equal(until(run, timeZone, now), expected, run);
});
