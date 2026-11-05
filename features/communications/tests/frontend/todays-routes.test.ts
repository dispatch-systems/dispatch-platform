import assert from 'node:assert/strict';
import test from 'node:test';
import type { Conversation, RouteDriver, Today } from '../../api/index.js';
import {
  driverToday,
  routeGroups,
  routeRows,
  routeToday,
} from '../../frontend/todays-routes/todays-routes.js';

/** A driver's work conversation, or a work phone's nobody held, waiting since `since` or
 * answered, last written in at `at`. */
const work = (id: string, at: number, since: number | null = null): Conversation => ({
  id,
  kind: 'work',
  teamId: null,
  number: '+12065550101',
  driverId: id,
  contact: false,
  added: false,
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

test('a driver’s work phone at the stop stands through the two hours after, though Today’s Routes, read since, lists no day', () => {
  const stopped: Today = {
    route: 'CX14',
    van: null,
    waveAt: 940,
    stops: 120,
    stopsDone: 120,
    pace: 'done',
    phone: '+12065550101',
  };
  // Their conversation, read half an hour after the stop, then Today's Routes a minute on.
  const read = { value: stopped, at: 1_000 };
  const over = { value: { day: null, drivers: [], phones: [] }, at: 61_000 };
  assert.equal(driverToday('avery', read, over)?.phone, '+12065550101');
  // As the two hours end, the conversation is read again, with none.
  assert.equal(driverToday('avery', { value: null, at: 7_200_000 }, over), null);
});

test('Recents shows a driver’s route on each of their conversations Driver Match knows them in, while they have a route today', () => {
  const as = (conversation: Conversation, code: string | null): Conversation => ({
    ...conversation,
    with: { ...conversation.with, code },
  });
  const on = (
    id: string,
    code: string | null,
    route: string | null,
    conversation?: Conversation,
  ) => {
    const each = driver(id, 940, conversation);
    return {
      ...each,
      person: { ...each.person, code },
      today: {
        ...each.today,
        route,
        stops: route === null ? null : 120,
        stopsDone: route === null ? null : 40,
      },
    };
  };
  const routes = {
    day: '2026-11-05',
    drivers: [
      on('avery', 'P1', 'CX1', work('avery', 500)),
      // No one Driver Match knows: their work conversation alone.
      on('blake', null, 'CX2', work('blake', 500)),
      // Gone from the day's list: no route, though their work conversation stays.
      on('casey', 'P3', null, work('casey', 500)),
    ],
    phones: [],
  };
  const routeOf = routeToday(routeGroups(routeRows(routes, [])));
  const shown = (conversation: Conversation) => routeOf(conversation)?.route ?? null;
  const personal = (id: string, code: string | null) =>
    as({ ...work(id, 600), kind: 'number', driverId: null }, code);
  const slack = (id: string, code: string | null) =>
    as({ ...work(id, 600), kind: 'slack', driverId: null, number: null }, code);
  assert.deepEqual(
    [
      shown(work('avery', 500)),
      shown(personal('avery-own', 'P1')),
      shown(slack('avery-dm', 'P1')),
      shown(work('blake', 500)),
      shown(work('casey', 500)),
      shown(personal('casey-own', 'P3')),
      // Matched, with no route today, and a number nobody linked.
      shown(personal('dana-own', 'P4')),
      shown(personal('stranger', null)),
    ],
    ['CX1', 'CX1', 'CX1', 'CX2', null, null, null, null],
  );
});
