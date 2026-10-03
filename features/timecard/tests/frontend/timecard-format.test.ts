import test from 'node:test';
import assert from 'node:assert/strict';
import { personName } from '../../frontend/meal-breaks.js';
import {
  hoursAndMinutes,
  punchTime,
  timecardDate,
  timecardPeriod,
} from '../../frontend/timecard-format.js';

test('employee timecards format local clocks and rounded minutes without timezone shifts', () => {
  assert.equal(hoursAndMinutes(8.5), '8h 30m');
  assert.equal(hoursAndMinutes(9.999), '10h 00m');
  assert.equal(hoursAndMinutes(0), '0h 00m');
  assert.equal(punchTime('00:05'), '12:05 AM');
  assert.equal(punchTime('12:00'), '12:00 PM');
  assert.equal(punchTime('17:30:00'), '5:30 PM');
  assert.equal(punchTime(null), '—');
  assert.equal(punchTime('Pending'), 'Pending');
  assert.equal(timecardDate('2026-09-19'), 'Sat, Sep 19');
  assert.match(timecardPeriod('2026-12-28', '2027-01-10'), /2026.*2027/);
});

test('names follow the chosen order and keep a provider’s own Last, First', () => {
  assert.equal(personName('Morgan, Alex', 'first_last'), 'Alex Morgan');
  assert.equal(personName('Morgan, Alex', 'last_first'), 'Morgan, Alex');
  assert.equal(personName('Alex  J Morgan', 'last_first'), 'Morgan, Alex J');
  assert.equal(personName('Alex  J Morgan', 'first_last'), 'Alex J Morgan');
  assert.equal(personName('Cher', 'last_first'), 'Cher');
  assert.equal(personName('Cher,', 'last_first'), 'Cher');
});
