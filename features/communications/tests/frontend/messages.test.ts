import assert from 'node:assert/strict';
import test from 'node:test';
import type { Message, Sender } from '../../api/index.js';
import { fresh, runs } from '../../frontend/conversations/messages.js';
import { latestAt, repliedSince, repliedText } from '../../frontend/conversations/replied.js';
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
    group: null,
    status: null,
    error: null,
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

test('the page shows pictures of the kinds browsers show, and offers others to download', () => {
  for (const kind of ['png', 'jpeg', 'gif', 'webp', 'avif']) assert.ok(isPhoto(`image/${kind}`));
  for (const kind of ['image/heic', 'image/heif', 'image/svg+xml', 'application/pdf'])
    assert.ok(!isPhoto(kind));
});

test('Send asks first once a teammate or the phone wrote after what the member started writing from', () => {
  const me = { ...phone('', 'Dana Cole'), userId: 'user_dana', phoneId: null };
  const keisha = { ...phone('', 'Keisha Brown'), userId: 'user_keisha', phoneId: null };
  const shown = [text('asked', 0), text('mine', 1, me)];
  const since = latestAt(shown);
  // Nothing since, or only the member's own, or the employee writing again: it sends.
  assert.equal(repliedSince(shown, since, 'user_dana'), undefined);
  const theirs = [...shown, text('mine again', 2, me), text('again', 3)];
  assert.equal(repliedSince(theirs, since, 'user_dana'), undefined);
  // A teammate's answer since, even with the employee's after it, the latest of them by when.
  const answered = [...theirs, text('on it', 4, keisha), text('thanks', 5)];
  const reply = repliedSince(answered, since, 'user_dana');
  assert.equal(reply?.id, 'on it');
  assert.equal(repliedText(reply!), 'Keisha just replied');
  // Already there as the member started, it's no news.
  assert.equal(repliedSince(answered, latestAt(answered), 'user_dana'), undefined);
  // Typed on the phone itself, by the phone's name.
  const fromPhone = repliedSince(
    [...shown, text('ok', 6, phone('phone_1', 'Dispatch 2'))],
    since,
    'user_dana',
  );
  assert.equal(repliedText(fromPhone!), 'Dispatch 2 just replied');
  // A teammate without a name on their account.
  const nameless = text('ok', 7, { ...keisha, name: '' });
  assert.equal(repliedText(nameless), 'Someone just replied');
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
