import assert from 'node:assert/strict';
import test from 'node:test';
import type { PeopleSource, Person, PersonId, Question } from '../../api/index.js';
import { shows } from '../../frontend/driver-match.js';
import { checkList } from '../../frontend/page/checks.js';

const id = (source: string, external: string, name: string): PersonId => ({
  source: source as PeopleSource,
  id: external,
  name,
  department: null,
  emails: [],
  phones: [],
  roles: [],
  listed: true,
  offboarded: false,
  photo: external === 'T1',
  firstSeen: null,
  lastSeen: null,
  linkedBy: 'new',
  confirmable: false,
});
const person = (code: string, role: Person['role'], ids: PersonId[]): Person => ({
  code,
  name: ids[0]!.name,
  role,
  title: null,
  given: null,
  ids,
  photo: 'v1',
});
const sources = ['paycom', 'amazon'] as PeopleSource[];

test('someone who has left shows only under Inactive or Offboarded, and a driver a source lacks is missing from it', () => {
  const gone = person('K7M2QX', 'former', [id('amazon', 'T9', 'Riley Quinn')]);
  const offboarded = person('P4TN8R', 'offboarded', [id('amazon', 'T8', 'Jesse Morales')]);
  const driver = person('W5HC2K', 'driver', [id('amazon', 'T2', 'Dmitri Volkov')]);
  const office = person('R39CFT', 'office', [id('paycom', 'E7', 'Avery Morgan')]);
  const everyone = [gone, offboarded, driver, office];
  assert.deepEqual(
    everyone.map((p) =>
      (['all', 'former', 'offboarded'] as const).map((filter) => shows(filter, p, sources)),
    ),
    [
      [false, true, false],
      [false, false, true],
      [true, false, false],
      [true, false, false],
    ],
  );
  // Only drivers and helpers belong in every source.
  assert.deepEqual(
    everyone.map((p) => shows('missing', p, sources)),
    [false, false, true, false],
  );
});

test('each question the backend asks is a check about the people it names', () => {
  const tony = person('K7M2QX', 'driver', [id('amazon', 'T1', 'Tony Reyes')]);
  const antonio = person('W5HC2K', 'driver', [
    id('paycom', 'E2', 'Antonio Reyes'),
    id('amazon', 'T4', 'Luis Hernandez'),
  ]);
  const questions: Question[] = [
    {
      kind: 'pair',
      person: tony.code,
      candidate: antonio.code,
      strength: 'strong',
      evidence: [
        {
          kind: 'short_form',
          short: 'Tony',
          long: 'Antonio',
          same: null,
          of: null,
          supports: true,
        },
        { kind: 'worked_days', short: null, long: null, same: 1, of: 4, supports: false },
      ],
    },
    { kind: 'variant', person: antonio.code, source: 'amazon' as PeopleSource, id: 'T4' },
    // Someone the page no longer lists asks nothing.
    { kind: 'variant', person: 'ZZZZZZ', source: 'amazon' as PeopleSource, id: 'T8' },
  ];
  const [pair, variant, ...rest] = checkList(questions, [tony, antonio]);
  assert.equal(rest.length, 0);
  assert.deepEqual(pair!.sides, [tony, antonio]);
  assert.deepEqual(pair!.reasons, [
    { text: 'Tony is short for Antonio', supports: true },
    { text: 'Worked 1 of 4 days together', supports: false },
  ]);
  assert.equal(pair!.likely, true);
  // A name variant sets the ID it asks about against everything else they hold, with the
  // photo on whichever side holds it.
  assert.deepEqual(
    variant!.sides.map((side) => [side.name, side.ids.map((i) => i.id), side.photo]),
    [
      ['Luis Hernandez', ['T4'], null],
      ['Antonio Reyes', ['E2'], null],
    ],
  );
  assert.deepEqual(variant!.codes, [antonio.code]);
});
