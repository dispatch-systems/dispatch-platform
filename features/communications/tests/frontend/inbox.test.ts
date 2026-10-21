import assert from 'node:assert/strict';
import test from 'node:test';
import type { Communications, CommunicationsWorkspace, Conversation } from '../../api/index.js';
import { marked, shownWorkspace, updated, withOlder } from '../../frontend/inbox.js';
import { initials, letter, waited } from '../../frontend/format.js';

/** A conversation, waiting since `since` or answered, its latest message written at `at`. */
const conversation = (
  id: string,
  at: number | null,
  since: number | null = null,
): Conversation => ({
  id,
  teamId: 'T0FIXTURE1',
  with: { userId: `U${id}`, name: id, code: null, role: null, photo: null },
  state: since === null ? 'responded' : 'waiting',
  waitingSince: since,
  lastAt: at,
  last: null,
  matched: [],
});
const ids = (inbox: Communications) => inbox.conversations.map((each) => each.id);

test('the list keeps the server’s order as it changes, and leaves older answered ones to be read in turn', () => {
  const inbox: Communications = {
    revision: 1,
    waiting: 2,
    conversations: [
      conversation('a', 300, 100),
      conversation('b', 400, 200),
      conversation('c', 900),
      conversation('d', 500),
    ],
    more: true,
    workspaces: [],
    typing: { version: 0, writing: [] },
  };
  const next = updated(inbox, {
    revision: 2,
    reset: false,
    waiting: 2,
    typing: { version: 0, writing: [] },
    workspaces: [],
    conversations: [
      // Waiting longest, it comes first.
      conversation('e', 60, 50),
      // Answered again, it stays first of the answered.
      conversation('c', 1000),
      // Done, it goes among the answered by when it was last written in.
      conversation('b', 600),
      // Older than the last listed while older ones are left: read with them.
      conversation('f', 100),
      // Opened, with nothing written yet.
      conversation('g', null),
    ],
  });
  assert.deepEqual(ids(next), ['e', 'a', 'c', 'b', 'd']);
  assert.equal(next.revision, 2);
  // Read in turn, older ones come after the last, each once.
  const older = withOlder(next, {
    conversations: [conversation('d', 500), conversation('f', 100), conversation('h', 50)],
    more: false,
  });
  assert.deepEqual(ids(older), ['e', 'a', 'c', 'b', 'd', 'f', 'h']);
  assert.equal(older.more, false);
});

test('the letters that matched are marked by the runs the server counts, UTF-16 code units', () => {
  const runs = (name: string, matched: [number, number][]) =>
    marked(name, matched).map(({ text, mark }) => (mark ? `[${text}]` : text));
  assert.deepEqual(
    runs('Jamal Williams', [
      [0, 4],
      [6, 10],
    ]),
    ['[Jama]', 'l ', '[Will]', 'iams'],
  );
  // A letter beyond the first plane counts two.
  assert.deepEqual(
    runs('𝒥o Núñez', [
      [0, 3],
      [4, 6],
    ]),
    ['[𝒥o]', ' ', '[Nú]', 'ñez'],
  );
});

test('a wait turns amber at 15 minutes and red at 30', () => {
  const minutes = (count: number) => waited(0, count * 60_000);
  assert.deepEqual(minutes(14), { label: '14m', tone: '' });
  assert.deepEqual(minutes(15), { label: '15m', tone: 'slow' });
  assert.deepEqual(minutes(30), { label: '30m', tone: 'late' });
  assert.deepEqual(minutes(125), { label: '2h', tone: 'late' });
});

test('the workspace chosen is shown while it is one of two or more where it works, else every one', () => {
  const workspace = (teamId: string, on = true): CommunicationsWorkspace => ({
    teamId,
    name: teamId,
    on,
    chatLink: '',
    needsUpdate: false,
    logo: null,
  });
  const both = [workspace('T1'), workspace('T2')];
  assert.equal(shownWorkspace('T2', both), 'T2');
  assert.equal(shownWorkspace(undefined, both), undefined);
  // Disconnected, switched off there, or the only one left.
  assert.equal(shownWorkspace('T3', both), undefined);
  assert.equal(shownWorkspace('T2', [workspace('T1'), workspace('T2', false)]), undefined);
  assert.equal(shownWorkspace('T2', [workspace('T2'), workspace('T3', false)]), undefined);
});

test('a contact is listed under their name’s first letter, and a workspace marked by its initials', () => {
  assert.deepEqual(['Ana Silva', 'émile Roy', 'Øyvind Berg', '2nd Shift', ' kevin'].map(letter), [
    'A',
    'E',
    'Ø',
    '#',
    'K',
  ]);
  assert.deepEqual(['BHL Drivers', 'Drivers', 'blue heron logistics', ''].map(initials), [
    'BD',
    'DR',
    'BH',
    '?',
  ]);
});
