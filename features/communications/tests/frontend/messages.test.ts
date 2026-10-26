import assert from 'node:assert/strict';
import test from 'node:test';
import type { Message } from '../../api/index.js';
import { runs } from '../../frontend/messages.js';

/** A text the DSP's phone `phone`, called `name`, sent from its own messaging app at `at`. */
const fromPhone = (id: string, phone: string, name: string, at: number): Message => ({
  id,
  ts: `${at}:${id}`,
  direction: 'out',
  text: id,
  at,
  editedAt: null,
  inThread: false,
  sender: { userId: null, phoneId: phone, name, code: null, photo: null },
  files: [],
  group: null,
  status: null,
});

test('texts two phones sent from their own apps minutes apart are a run under each phone', () => {
  const at = Date.UTC(2026, 9, 26, 16);
  const [day] = runs(
    [
      fromPhone('t1', 'phone_dispatch', 'Dispatch', at),
      fromPhone('t2', 'phone_station', 'Station 2', at + 60_000),
      fromPhone('t3', 'phone_station', 'Station 2', at + 120_000),
    ],
    'America/Los_Angeles',
  );
  assert.deepEqual(
    day!.runs.map((run) => run.messages.map((message) => message.id)),
    [['t1'], ['t2', 't3']],
  );
});
