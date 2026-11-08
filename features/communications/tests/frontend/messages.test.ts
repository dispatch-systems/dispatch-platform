import assert from 'node:assert/strict';
import test from 'node:test';
import type { Message, Sender } from '../../api/index.js';
import { fresh, runs } from '../../frontend/conversations/messages.js';
import { Replies, repliedText } from '../../frontend/conversations/replied.js';
import { isPhoto, whyNotSent } from '../../frontend/shared/format.js';

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
    gif: null,
    group: null,
    status: null,
    error: null,
    number: '+14255550199',
    moved: false,
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

test('sender labels remember the last outgoing sender across replies, pauses and a day boundary', () => {
  const [dispatch, station] = [phone('dispatch', 'Dispatch'), phone('station', 'Station')];
  const days = runs(
    [
      text('first', 0, dispatch),
      text('reply', 1),
      text('again', 20, dispatch),
      text('reply again', 21),
      text('other phone', 22, station),
      text('next day', 24 * 60, station),
    ],
    'America/Los_Angeles',
  );
  assert.deepEqual(
    days.map((day) => day.runs.map((run) => run.sentBefore?.phoneId)),
    [[undefined, 'dispatch', 'dispatch', 'dispatch', 'dispatch'], [undefined]],
  );
});

test('the page shows pictures of the kinds browsers show, and offers others to download', () => {
  for (const kind of ['png', 'jpeg', 'gif', 'webp', 'avif']) assert.ok(isPhoto(`image/${kind}`));
  for (const kind of ['image/heic', 'image/heif', 'image/svg+xml', 'application/pdf'])
    assert.ok(!isPhoto(kind));
});

test('Send asks first once a teammate or the phone wrote what the page didn’t hold as the member started writing', () => {
  const me = { ...phone('', 'Dana Cole'), userId: 'user_dana', phoneId: null };
  const keisha = { ...phone('', 'Keisha Brown'), userId: 'user_keisha', phoneId: null };
  const replies = new Replies('user_dana');
  const read = [text('asked', 10), text('mine', 11, me)];
  // Told the member saw it before it's read, it waits for the read to count from.
  replies.seen();
  replies.hold(read);
  // Nothing since, or only the member's own, or the employee writing again: it sends.
  assert.equal(replies.check(), undefined);
  replies.hold([...read, text('mine again', 12, me)]);
  replies.hold([...read, text('again', 13)]);
  assert.equal(replies.check(), undefined);
  // A teammate's answer, as an update's last message, the latest of them by when.
  replies.hold([...read, text('on it', 14, keisha)]);
  assert.equal(replies.check(), 'Keisha just replied');
  // The next update's last is the employee's, before the conversation is read again: the reply
  // is still news.
  replies.hold([...read, text('thanks', 15)]);
  assert.equal(replies.check(), 'Keisha just replied');
  // Seen, it's no news, nor once the read that holds it comes.
  replies.seen();
  assert.equal(replies.check(), undefined);
  replies.hold([...read, text('on it', 14, keisha), text('thanks', 15)]);
  assert.equal(replies.check(), undefined);
  // Typed on the phone itself, by the phone's name.
  replies.hold([...read, text('ok', 16, phone('phone_1', 'Dispatch 2'))]);
  assert.equal(replies.check(), 'Dispatch 2 just replied');
  // A teammate without a name on their account.
  assert.equal(repliedText(text('ok', 17, { ...keisha, name: '' })), 'Someone just replied');
});

test('a first text stamped minutes ahead by its phone doesn’t hide a teammate’s reply', () => {
  const keisha = { ...phone('', 'Keisha Brown'), userId: 'user_keisha', phoneId: null };
  const replies = new Replies('user_dana');
  const first = text('back on route', 45);
  replies.hold([first]);
  replies.hold([first, text('calling them', 16, keisha)]);
  assert.equal(replies.check(), 'Keisha just replied');
});

test('a text that failed says why in a few words: the server’s codes worded, the phone’s as it wrote them', () => {
  assert.equal(whyNotSent('phones_offline'), 'Phone offline');
  assert.equal(whyNotSent('expired'), 'Phone offline');
  assert.equal(whyNotSent('phones_removed'), 'Phone removed');
  assert.equal(whyNotSent('No service'), 'No service');
  // None, or a code the page doesn't know, says nothing beside Not sent.
  assert.equal(whyNotSent(null), '');
  assert.equal(whyNotSent('phones_something_new'), '');
});
