import assert from 'node:assert/strict';
import test from 'node:test';
import type { Message, Sender } from '../../api/index.js';
import { fresh, runs } from '../../frontend/messages.js';

/** A text written `minutes` past 4 PM: from the number a conversation is with, or by `sender`. */
const text = (id: string, minutes: number, sender: Sender | null = null): Message => {
  const at = Date.UTC(2026, 9, 26, 16, minutes);
  return {
    id,
    ts: `${at}:${id}`,
    direction: sender ? 'out' : 'in',
    text: id,
    at,
    editedAt: null,
    inThread: false,
    sender,
    files: [],
    group: null,
    status: null,
    number: '+14255550199',
  };
};
/** The DSP's phone `id`, called `name`, sending from its own messaging app. */
const phone = (id: string, name: string): Sender => ({
  userId: null,
  phoneId: id,
  name,
  code: null,
  photo: null,
});
const ids = (messages: readonly Message[]) => messages.map((message) => message.id);

test('a conversation read again holds only what it holds now, and what the page just sent', () => {
  const [o1, o2, n1, n2] = [text('o1', 0), text('o2', 1), text('n1', 2), text('n2', 3)];
  const none = () => false;
  // Its texts filed elsewhere, those of an older page read before too, it's empty.
  const emptied = fresh(
    { messages: [o1, n1, n2], more: false },
    { messages: [], more: false },
    none,
  );
  assert.deepEqual([ids(emptied.messages), emptied.more], [[], false]);
  // One of the latest filed elsewhere, those before what's read again stay as they were.
  const moved = fresh({ messages: [o1, o2, n1], more: true }, { messages: [o2], more: true }, none);
  assert.deepEqual([ids(moved.messages), moved.more], [['o1', 'o2'], true]);
  // A text the page sent after the read was asked stays, though the read doesn't hold it.
  const sending = fresh(
    { messages: [o2, n1], more: false },
    { messages: [o2], more: false },
    (message) => message.id === 'n1',
  );
  assert.deepEqual(ids(sending.messages), ['o2', 'n1']);
});

test('texts two phones sent from their own apps minutes apart are a run under each phone', () => {
  const [dispatch, station] = [
    phone('phone_dispatch', 'Dispatch'),
    phone('phone_station', 'Station 2'),
  ];
  const [day] = runs(
    [text('t1', 0, dispatch), text('t2', 1, station), text('t3', 2, station)],
    'America/Los_Angeles',
  );
  assert.deepEqual(
    day!.runs.map((run) => ids(run.messages)),
    [['t1'], ['t2', 't3']],
  );
});
