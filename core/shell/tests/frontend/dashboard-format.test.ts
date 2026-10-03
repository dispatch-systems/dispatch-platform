import test from 'node:test';
import assert from 'node:assert/strict';
import {
  bytes,
  countdown,
  duration,
  elapsed,
  time,
  timeOfDay,
  timeWithSeconds,
  title,
  utcDay,
} from '../../frontend/lib/format.js';
import { backoff } from '../../frontend/lib/backoff.js';
import { messageOf } from '../../frontend/lib/errors.js';
import { sourceLink } from '../../frontend/lib/source.js';

test('timestamps read in the timezone they are given', () => {
  assert.equal(time('2026-09-18T15:03:25Z', 'America/Chicago'), 'Sep 18, 10:03 AM');
  assert.equal(time('2026-09-18T15:03:25Z', 'UTC'), 'Sep 18, 3:03 PM');
  assert.equal(timeOfDay('2026-09-18T15:03:25Z', 'America/Los_Angeles'), '8:03 AM');
  assert.equal(timeOfDay('2000-01-01T00:00:00Z', 'UTC'), '12:00 AM');
  // A log with many entries a minute reads to the second.
  assert.equal(timeWithSeconds('2026-09-18T15:03:25Z', 'America/Chicago'), 'Sep 18, 10:03:25 AM');
  assert.equal(timeWithSeconds('2026-09-18T00:00:07.900Z', 'UTC'), 'Sep 18, 12:00:07 AM');
  // A UTC day, whatever the viewer's clock, with its year only outside the current UTC year.
  const now = Date.parse('2026-01-01T02:00:00Z');
  assert.equal(utcDay('2026-01-01T00:30:00Z', now), 'Jan 1');
  assert.equal(utcDay('2025-12-31T23:59:59Z', now), 'Dec 31, 2025');
});

test('a missing timestamp reads Never unless the caller words it', () => {
  assert.equal(time(null, 'UTC'), 'Never');
  assert.equal(time(undefined, 'UTC'), 'Never');
  assert.equal(time(null, 'UTC', 'Not scheduled'), 'Not scheduled');
});

test('identifiers become titles', () => {
  assert.equal(title('waiting_verification'), 'Waiting Verification');
  assert.equal(title('dsp.view_opened'), 'Dsp View Opened');
});

test('measured durations keep their precision and recorded seconds stay whole', () => {
  assert.equal(duration(null), '—');
  assert.equal(duration(999.6), '1000 ms');
  assert.equal(duration(45_000), '45.0 s');
  assert.equal(duration(200_000), '3m 20s');
  assert.equal(elapsed(45), '45s');
  assert.equal(elapsed(200), '3m 20s');
});

test('a countdown reads minutes and seconds left, and stops at 0:00', () => {
  const now = Date.parse('2026-10-02T15:00:00Z');
  assert.equal(countdown('2026-10-02T15:10:00Z', now), '10:00');
  assert.equal(countdown('2026-10-02T15:09:42Z', now), '9:42');
  // A part second still left counts as a second.
  assert.equal(countdown('2026-10-02T15:00:05.200Z', now), '0:06');
  assert.equal(countdown('2026-10-02T14:59:00Z', now), '0:00');
});

test('byte counts use binary units', () => {
  assert.equal(bytes(700 * 1024 ** 2, 'MiB'), '700 MiB');
  assert.equal(bytes(700.5 * 1024 ** 2, 'MiB'), '701 MiB');
  assert.equal(bytes(734_003_200, 'MiB', 1), '700.0 MiB');
  assert.equal(bytes(1.26 * 1024 ** 3, 'GiB', 1), '1.3 GiB');
});

test('retries back off from one second to fifteen', () => {
  assert.deepEqual([0, 1, 2, 3, 4, 9].map(backoff), [1000, 2000, 4000, 8000, 15000, 15000]);
});

test('an error message falls back only when there is none', () => {
  assert.equal(
    messageOf(new Error('Keep at least one DSP owner.')),
    'Keep at least one DSP owner.',
  );
  assert.equal(messageOf('nope'), 'The request could not be completed.');
  assert.equal(messageOf(new Error('')), 'The request could not be completed.');
});

test('the source link names the release on Production and the commit elsewhere', () => {
  const repository = 'https://github.com/dispatch-systems/dispatch-platform';
  const commit = 'c3f898be7c709f65c9d6e69a391fffbe03ecb5b6';
  assert.deepEqual(sourceLink({ version: '0.0.23', commit }), {
    href: `${repository}/tree/v0.0.23`,
    label: 'v0.0.23',
  });
  assert.deepEqual(sourceLink({ version: null, commit }), {
    href: `${repository}/tree/${commit}`,
    label: 'c3f898b',
  });
  assert.deepEqual(sourceLink({ version: null, commit: null }), { href: repository });
});
