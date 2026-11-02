import assert from 'node:assert/strict';
import test from 'node:test';
import type { Conversation, RouteDriver, Today } from '../../api/index.js';
import { routeGroups, routeRows } from '../../frontend/todays-routes/todays-routes.js';

/** A driver's work conversation, or a work phone's nobody held, waiting since `since` or
 * answered, last written in at `at`. */
const work = (id: string, at: number, since: number | null = null): Conversation => ({
  id,
  kind: 'work',
  teamId: null,
  number: '+12065550101',
  driverId: id,
  contact: false,
  blocked: false,
  with: { userId: null, name: id, code: null, role: null, title: null, photo: null },
  state: since === null ? 'responded' : 'waiting',
  waitingSince: since,
  lastAt: at,
  last: null,
  matched: [],
  groupId: null,
});
/** A driver on a route today, its wave starting at `wave`, with their conversation, if any. */
const driver = (id: string, wave: number | null, conversation?: Conversation): RouteDriver => {
  const today: Today = {
    route: null,
    van: null,
    waveAt: wave,
    stops: null,
    stopsDone: null,
    pace: null,
    phone: null,
  };
  return {
    driverId: id,
    person: { userId: null, name: id, code: null, role: null, title: null, photo: null },
    today,
    conversation: conversation ?? null,
  };
};

test('Today’s Routes lists its drivers by name, waiting or not, then the work phones nobody held, the latest written in first, each with its latest conversation', () => {
  const routes = {
    day: '2026-10-26',
    // By name, as the server lists them.
    drivers: [
      driver('avery', 940, work('avery', 500, 300)),
      driver('blake', 920),
      driver('casey', 940, work('casey', 700)),
      driver('dana', null),
      driver('evan', 920, work('evan', 400)),
    ],
    // The latest written in first, as the server read them.
    phones: [
      { ...work('phone1', 600), driverId: null },
      { ...work('phone2', 550), driverId: null },
    ],
  };
  // The list's own copies are the latest: Evan's now waits, and the second phone was written in
  // since.
  const listed = [work('evan', 800, 200), { ...work('phone2', 900, 850), driverId: null }];
  const groups = routeGroups(routeRows(routes, listed)).map(({ key, rows }) => [
    key,
    rows.map((row) => [
      row.conversation?.id ?? (row.kind === 'driver' ? row.driver.driverId : ''),
      row.conversation?.state ?? null,
    ]),
  ]);
  assert.deepEqual(groups, [
    [
      'drivers',
      [
        ['avery', 'waiting'],
        ['blake', null],
        ['casey', 'responded'],
        ['dana', null],
        ['evan', 'waiting'],
      ],
    ],
    [
      'no_driver',
      [
        ['phone2', 'waiting'],
        ['phone1', 'responded'],
      ],
    ],
  ]);
});
