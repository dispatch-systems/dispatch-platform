import assert from 'node:assert/strict';
import test from 'node:test';
import type {
  Communications,
  CommunicationsWorkspace,
  Contact,
  Conversation,
  Elsewhere,
  Group,
  GroupChoice,
  Reachable,
} from '../../api/index.js';
import {
  rows,
  shownWorkspace,
  updated,
  withOlder,
  type Section,
} from '../../frontend/inbox/inbox.js';
import { elsewhereNow } from '../../frontend/conversations/elsewhere.js';
import { groupName, recipients, type Pick } from '../../frontend/slack/groups/groups.js';
import { letter, waited } from '../../frontend/shared/format.js';
import { marked } from '../../frontend/shared/marked.js';

/** A conversation, waiting since `since` or answered, its latest message written at `at`, listed
 * under the group `group`, if any. */
const conversation = (
  id: string,
  at: number | null,
  since: number | null = null,
  group: string | null = null,
): Conversation => ({
  id,
  kind: 'slack',
  teamId: 'T0FIXTURE1',
  number: null,
  driverId: null,
  contact: false,
  blocked: false,
  with: { userId: `U${id}`, name: id, code: null, role: null, title: null, photo: null },
  state: since === null ? 'responded' : 'waiting',
  waitingSince: since,
  lastAt: at,
  last: null,
  matched: [],
  groupId: group,
});
/** A group, waiting since `since` or answered, last written in at `at`. */
const group = (id: string, at: number, since: number | null = null): Group => ({
  id,
  name: id,
  people: 2,
  faces: [],
  teamIds: ['T0FIXTURE1'],
  state: since === null ? 'responded' : 'waiting',
  waitingSince: since,
  lastAt: at,
  last: null,
});
/** A number's conversation by text, as `conversation`'s. */
const texts = (id: string, at: number, since: number | null = null): Conversation => ({
  ...conversation(id, at, since),
  kind: 'number',
  teamId: null,
  number: '+14255550199',
  with: { userId: null, name: id, code: null, role: null, title: null, photo: null },
});
const ids = (inbox: Communications, section: Section = 'slack') =>
  rows(inbox, section).map((each) => each.row.id);

test('each section’s list keeps the server’s order as it changes, with the phones and Today’s Routes as each update brings them, and leaves its older answered ones to be read in turn', () => {
  const inbox: Communications = {
    revision: 1,
    waiting: 2,
    conversations: [
      conversation('a', 300, 100),
      conversation('b', 400, 200),
      conversation('c', 900),
      conversation('d', 500),
      texts('n0', 600),
    ],
    groups: [group('g1', 800)],
    more: true,
    moreTexts: true,
    workspaces: [],
    texting: true,
    routes: false,
    phones: [],
    typing: { version: 0, writing: [] },
  };
  // The phone went offline, and live route data began.
  const offline = { id: 'phone', name: 'Dispatch', online: false, lastSeenAt: 900 };
  const next = updated(inbox, {
    revision: 2,
    reset: false,
    waiting: 3,
    typing: { version: 0, writing: [] },
    workspaces: [],
    texting: true,
    routes: true,
    phones: [offline],
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
      // Listed under a group, the group's row stands for it.
      conversation('y', 80, 70, 'g2'),
    ],
    // A group waits among the conversations by its longest wait.
    groups: [group('g2', 80, 70)],
  });
  assert.deepEqual(ids(next), ['e', 'g2', 'a', 'c', 'g1', 'b', 'd']);
  assert.deepEqual([next.revision, next.routes, next.phones], [2, true, [offline]]);
  // Read in turn, older ones come after the last, each once, groups among them.
  const older = withOlder(
    next,
    {
      conversations: [conversation('d', 500), conversation('f', 100), conversation('h', 50)],
      groups: [group('g0', 300)],
      more: false,
    },
    'slack',
  );
  assert.deepEqual(ids(older), ['e', 'g2', 'a', 'c', 'g1', 'b', 'd', 'g0', 'f', 'h']);
  assert.equal(older.more, false);
  // Messaging's go on from its own last answered, whatever Slack's has read.
  const texted = updated(older, {
    ...older,
    reset: false,
    conversations: [texts('n1', 950, 940), texts('n2', 20)],
    groups: [],
  });
  assert.deepEqual(ids(texted, 'messaging'), ['n1', 'n0']);
  const read = withOlder(
    texted,
    { conversations: [texts('n2', 20)], groups: [], more: false },
    'messaging',
  );
  assert.deepEqual(ids(read, 'messaging'), ['n1', 'n0', 'n2']);
  assert.deepEqual(ids(read), ids(older));
});

test('a conversation’s chips follow the list as it changes: a dot while each other waits, one started since by whom it’s with', () => {
  const chip = (kind: Elsewhere['kind'], conversationId: string | null): Elsewhere => ({
    kind,
    conversationId,
    waiting: false,
    number: kind === 'number' ? '+14255550199' : null,
    driverId: null,
    teamId: kind === 'slack' ? 'T0FIXTURE1' : null,
    userId: kind === 'slack' ? 'Us1' : null,
  });
  // As read with the work conversation open: their Messaging one answered, no Slack one yet.
  const read = [chip('work', 'w1'), chip('number', 'n1'), chip('slack', null)];
  const waits = (listed: Conversation[]) =>
    elsewhereNow(read, listed).map((each) => [each.conversationId, each.waiting]);
  assert.deepEqual(waits([]), [
    ['w1', false],
    ['n1', false],
    [null, false],
  ]);
  // A text comes in Messaging, and they write first in Slack.
  assert.deepEqual(waits([texts('n1', 600, 590), conversation('s1', 700, 650)]), [
    ['w1', false],
    ['n1', true],
    ['s1', true],
  ]);
  // Both answered elsewhere, neither waits.
  assert.deepEqual(waits([texts('n1', 800), conversation('s1', 900)]), [
    ['w1', false],
    ['n1', false],
    ['s1', false],
  ]);
});

test('picks go to each person once: groups narrowed by the workspaces picked, someone by name always', () => {
  const person = (key: string, ...teams: string[]): Reachable => ({
    key,
    person: { userId: `U${key}`, name: key, code: key, role: null, title: null, photo: null },
    members: teams.map((teamId) => ({ teamId, userId: `U${key}` })),
  });
  const people = [person('ana', 'T1'), person('bo', 'T2'), person('cy', 'T1', 'T2')];
  const choice = (kind: GroupChoice['kind'], keys: string[], teamId: string | null = null) => ({
    kind: 'group' as const,
    key: `${kind}:${teamId}`,
    choice: { kind, waveAt: null, role: null, teamId, people: keys },
  });
  const drivers = choice('role', ['ana', 'bo', 'cy']);
  const second = choice('workspace', ['bo', 'cy'], 'T2');
  const everyone = choice('everyone', ['ana', 'bo', 'cy']);
  const sent = (...picks: Pick[]) =>
    recipients(picks, people).map(({ teamId, userId }) => `${teamId}:${userId}`);
  assert.deepEqual(sent(drivers), ['T1:Uana', 'T2:Ubo', 'T1:Ucy']);
  // A workspace narrows the others to it, by the account there; alone, it's everyone there.
  assert.deepEqual(sent(drivers, second), ['T2:Ubo', 'T2:Ucy']);
  assert.deepEqual(sent(second), ['T2:Ubo', 'T2:Ucy']);
  assert.deepEqual(sent(everyone, drivers), ['T1:Uana', 'T2:Ubo', 'T1:Ucy']);
  // Someone by name goes as picked, once.
  const contact = { teamId: 'T2', person: { userId: 'Ucy', code: 'cy' } } as Contact;
  const byName: Pick = { kind: 'person', key: 'cy', contact };
  assert.deepEqual(sent(choice('role', ['ana', 'cy']), byName), ['T1:Uana', 'T2:Ucy']);
  assert.deepEqual(sent(), []);
});

test('a group is named for what was picked', () => {
  const named = (pick: Pick) => pick.key;
  const person = (name: string): Pick => ({
    kind: 'person',
    key: name,
    contact: { person: { name } } as Contact,
  });
  const wave: Pick = { kind: 'group', key: '9:20 AM wave', choice: {} as GroupChoice };
  assert.equal(groupName([wave], named), '9:20 AM wave');
  assert.equal(groupName([person('Andre Thompson')], named), 'Andre Thompson');
  assert.equal(groupName([wave, person('Andre Thompson')], named), '9:20 AM wave and Andre');
  const five = ['Andre T', 'Carlos M', 'Tyler N', 'Dustin H', 'Anna S'].map(person);
  assert.equal(groupName(five, named), 'Andre, Carlos and 3 others');
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

test('a contact is listed under their name’s first letter', () => {
  assert.deepEqual(['Ana Silva', 'émile Roy', 'Øyvind Berg', '2nd Shift', ' kevin'].map(letter), [
    'A',
    'E',
    'Ø',
    '#',
    'K',
  ]);
});
