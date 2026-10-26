import assert from 'node:assert/strict';
import test, { type TestContext } from 'node:test';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
import { dataCache } from '../../../../core/shell/frontend/runtime/data-cache.js';
import { settingsRead, type Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import { waitingUrl } from '../../api/client.js';
import type { WaitingMessage } from '../../api/index.js';
import { alertMember } from '../../frontend/alerts/alerting.js';
import {
  arrived,
  keepSeen,
  readSeen,
  SEEN_MS,
  type InFront,
  type Seen,
} from '../../frontend/alerts/arrivals.js';

/** The message `id` waiting in the conversation `conversation`, listed under `group`, if any. */
const waits = (conversation: string, id: string, group: string | null = null): WaitingMessage => ({
  conversationId: conversation,
  groupId: group,
  from: conversation,
  message: {
    id,
    ts: '1761400000.000100',
    direction: 'in',
    text: 'Running late',
    at: 0,
    editedAt: null,
    inThread: false,
    sender: null,
    files: [],
    group: null,
  },
});

/**
 * The member's tabs at a DSP, sharing what they saw as the browser keeps it: `looks` is the tab
 * alerting looking at what waits, as its own copy has it, at `now`, answering what alerts.
 */
function browser() {
  let seen: Seen | undefined;
  return (latest: WaitingMessage[], now: number, inFront?: InFront) => {
    const looked = arrived(latest, seen, now, inFront);
    seen = looked.seen;
    return looked.fresh.map((each) => each.message.id);
  };
}

test('a new message from an employee alerts once, whichever tab looks, and none from before', () => {
  const looks = browser();
  // What waits as Dispatch opens alerts no one.
  assert.deepEqual(looks([waits('jordan', 'm1')], 0), []);
  // A conversation that starts waiting, then another message in one waiting, each alert once.
  assert.deepEqual(looks([waits('jordan', 'm1'), waits('casey', 'm2')], 5_000), ['m2']);
  assert.deepEqual(looks([waits('jordan', 'm1'), waits('casey', 'm2')], 10_000), []);
  assert.deepEqual(looks([waits('jordan', 'm3'), waits('casey', 'm2')], 15_000), ['m3']);
  // Another tab, alerting now, reads an older copy of what waits, then a newer: neither alerts.
  assert.deepEqual(looks([waits('jordan', 'm1'), waits('casey', 'm2')], 20_000), []);
  assert.deepEqual(looks([waits('jordan', 'm3'), waits('casey', 'm2')], 25_000), []);
  // Answered, Casey's goes; written to again, the new message alerts.
  assert.deepEqual(looks([waits('jordan', 'm3')], 30_000), []);
  assert.deepEqual(looks([waits('jordan', 'm3'), waits('casey', 'm4')], 35_000), ['m4']);
  // With every tab closed a while, what came meanwhile is from before Dispatch opened again.
  const reopened = 35_000 + SEEN_MS + 1;
  assert.deepEqual(looks([waits('jordan', 'm3'), waits('casey', 'm5')], reopened), []);
});

test('a message in front of the member alerts no one, then or later', () => {
  const looks = browser();
  looks([], 0);
  // The conversation open on the page, or the group it's listed under.
  assert.deepEqual(looks([waits('jordan', 'm1')], 5_000, { conversation: 'jordan' }), []);
  const grouped = [waits('jordan', 'm1'), waits('casey', 'm2', 'drivers')];
  assert.deepEqual(looks(grouped, 10_000, { group: 'drivers' }), []);
  assert.deepEqual(looks(grouped, 15_000), []);
  // Another conversation's still does.
  assert.deepEqual(looks([...grouped, waits('sam', 'm3')], 20_000, { group: 'drivers' }), ['m3']);
});

test('every message waiting is remembered, however many wait', () => {
  const looks = browser();
  const many = Array.from({ length: 501 }, (_, n) => waits(`driver${n}`, `m${n}`));
  looks(many, 0);
  assert.deepEqual(looks(many, 5_000), []);
  assert.deepEqual(looks(many, 10_000), []);
});

/** The member's choices of how a new message alerts them, as Settings answers them. */
const chose = (notifications: boolean) =>
  ({
    pages: [],
    sections: [{ id: 'communications.display', values: { sound: false, notifications } }],
  }) as unknown as SettingsAnswer;
/** Until what's queued has had its turn. */
const turns = () => new Promise((resolve) => setImmediate(resolve));

/**
 * Web Locks as a browser has them, for the one lock the member's tabs take turns with, which
 * Node 22 lacks: held by one tab at a time, the others waiting in line unless they give up their
 * place; a tab stealing it takes it at once, the one holding it losing it.
 */
function lockManager() {
  let holding: object | undefined;
  const line: (() => void)[] = [];
  const lost = (reject: (error: unknown) => void) => () =>
    reject(new DOMException('The lock was taken', 'AbortError'));
  let losing = () => {};
  return {
    request(
      _name: string,
      options: { steal?: boolean; signal?: AbortSignal },
      held: () => unknown,
    ) {
      return new Promise((resolve, reject) => {
        const grant = () => {
          const holder = {};
          holding = holder;
          losing = lost(reject);
          queueMicrotask(() => {
            if (holding !== holder) return;
            void Promise.resolve(held())
              .finally(() => {
                if (holding !== holder) return;
                holding = undefined;
                line.shift()?.();
              })
              .then(resolve, reject);
          });
        };
        if (options.steal) {
          if (holding) losing();
          grant();
        } else if (!holding) grant();
        else {
          line.push(grant);
          options.signal?.addEventListener('abort', () => {
            const place = line.indexOf(grant);
            if (place < 0) return;
            line.splice(place, 1);
            reject(new DOMException('Its place was given up', 'AbortError'));
          });
        }
      });
    },
  };
}

/**
 * A tab of `user`'s at a DSP, in front of them if `focused`, in a browser the test holds: its
 * clock, which the test moves; the server, answering Settings and what waits as `server` says,
 * or holding back the next answer to `url` till the test gives it (`hold`); the notifications it
 * shows; and the member's other tabs, sharing its storage and taking turns alerting with it
 * (`other`), the focused one from it, else as the next in line.
 */
function tab(t: TestContext, user: string, focused: boolean) {
  t.mock.timers.enable({ apis: ['setInterval', 'Date'], now: 1_761_400_000_000 });
  const dsp = 'dsp_north';
  const server = { settings: chose(false), waiting: [] as WaitingMessage[] };
  const held = new Map<string, (give: (answer: unknown) => void) => void>();
  const answer = (url: string) =>
    url === settingsRead
      ? server.settings
      : { revision: 0, count: server.waiting.length, since: [], latest: server.waiting };
  t.mock.method(globalThis, 'fetch', async (url: string) => {
    const holding = held.get(url);
    held.delete(url);
    return Response.json(holding ? await new Promise((give) => holding(give)) : answer(url));
  });
  const hold = (url: string) =>
    new Promise<(answer: unknown) => void>((asked) => held.set(url, asked));
  const shown: string[] = [];
  const window = new EventTarget();
  const location = { hash: `#dsp/${dsp}/dashboard` };
  const storage = new Map<string, string>();
  const browser = {
    window: Object.assign(window, { location }),
    location,
    addEventListener: window.addEventListener.bind(window),
    removeEventListener: window.removeEventListener.bind(window),
    document: Object.assign(new EventTarget(), {
      visibilityState: focused ? 'visible' : 'hidden',
      hasFocus: () => focused,
    }),
    localStorage: {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
        window.dispatchEvent(new Event('seen'));
      },
    },
    Notification: class {
      static permission = 'granted';
      constructor(title: string, { body }: { body: string }) {
        shown.push(`${title}: ${body}`);
      }
    },
  };
  Object.assign(globalThis, browser);
  const node = Object.getOwnPropertyDescriptor(globalThis, 'navigator')!;
  Object.defineProperty(globalThis, 'navigator', {
    value: { locks: lockManager() },
    configurable: true,
  });
  // What the test opened, closed the last first, before the browser goes.
  const opened: (() => void)[] = [];
  t.after(() => {
    for (const close of opened.reverse()) close();
    for (const each of Object.keys(browser)) delete (globalThis as Record<string, unknown>)[each];
    Object.defineProperty(globalThis, 'navigator', node);
    dataCache.clear();
  });
  const access = { view: { dsp: { id: dsp } }, session: { user: { id: user } } } as Watching;
  /** The member's tabs saw `ids` waiting, just now. */
  const saw = (...ids: string[]) => keepSeen(dsp, user, { at: Date.now(), ids });
  /** Until the tab alerting has seen the message `id`, if it hasn't yet. */
  const sees = (id: string) =>
    new Promise<void>((seen) => {
      const looked = () => {
        if (!readSeen(dsp, user)?.ids.includes(id)) return;
        window.removeEventListener('seen', looked);
        seen();
      };
      window.addEventListener('seen', looked);
      looked();
    });
  const other = (focused: boolean) =>
    new Promise<() => void>((taken) => {
      void navigator.locks.request(
        `dispatch-communications-alerts:${dsp}:${user}`,
        focused ? { steal: true } : {},
        () =>
          new Promise<void>((closes) => {
            opened.push(closes);
            taken(closes);
          }),
      );
    });
  /** The tab starts alerting the member. */
  const start = () => opened.push(alertMember(() => access));
  return { server, hold, shown, saw, sees, other, start };
}

test('a tab taking its turn alerts as the member last chose, in whichever tab they chose it', async (t) => {
  const { server, shown, saw, sees, other, start } = tab(t, 'user_handoff', false);
  // This tab read Settings with notifications on; another, in front of the member, alerts them.
  dataCache.put(settingsRead, chose(true));
  const close = await other(true);
  saw('m1');
  start();
  // There they turn notifications off, and close it as Casey writes.
  server.settings = chose(false);
  server.waiting = [waits('jordan', 'm1'), waits('casey', 'm2')];
  close();
  await sees('m2');
  // On again here, as Settings saves them, Sam's message alerts.
  dataCache.put(settingsRead, chose(true));
  server.waiting = [...server.waiting, waits('sam', 'm3')];
  t.mock.timers.tick(30_000);
  await sees('m3');
  assert.deepEqual(shown, ['sam: Running late']);
});

test("a turn reads what the member chose itself, not as a read begun in the tab's turn before has it", async (t) => {
  const { server, hold, shown, saw, sees, other, start } = tab(t, 'user_rereads', true);
  server.settings = chose(true);
  server.waiting = [waits('jordan', 'm1'), waits('casey', 'm2')];
  saw('m1');
  const asked = hold(settingsRead);
  start();
  const give = await asked;
  // Before it's read, the member focuses another tab, turns notifications off there, and closes
  // it; what this tab was reading comes after.
  const close = await other(true);
  server.settings = chose(false);
  close();
  await turns();
  give(chose(true));
  await sees('m2');
  assert.deepEqual(shown, []);
});

test('what a tab reads for a turn that ended alerts no one', async (t) => {
  const { server, hold, shown, saw, sees, other, start } = tab(t, 'user_turns', true);
  server.settings = chose(true);
  server.waiting = [waits('jordan', 'm1'), waits('casey', 'm2')];
  saw('m1');
  start();
  await sees('m2');
  // Half a minute on, it reads what waits again; meanwhile the member focuses another tab.
  const asked = hold(waitingUrl);
  t.mock.timers.tick(30_000);
  const give = await asked;
  const close = await other(true);
  await turns();
  // Sam's message comes to this tab, no longer alerting: the other, with Sam's open, saw it.
  give({ revision: 0, count: 3, since: [], latest: [...server.waiting, waits('sam', 'm3')] });
  await turns();
  saw('m1', 'm2', 'm3');
  // Its turn again as the other closes, Alex's message alerts.
  server.waiting = [...server.waiting, waits('sam', 'm3'), waits('alex', 'm4')];
  t.mock.timers.tick(30_000);
  close();
  await sees('m4');
  assert.deepEqual(shown, ['casey: Running late', 'alex: Running late']);
});

test('what waits as a tab starts alerting alerts as the member chose, once that is read', async (t) => {
  const { server, hold, shown, saw, sees, start } = tab(t, 'user_reload', true);
  server.waiting = [waits('jordan', 'm1'), waits('casey', 'm2')];
  saw('m1');
  const asked = hold(settingsRead);
  start();
  const give = await asked;
  await turns();
  give(chose(true));
  await sees('m2');
  assert.deepEqual(shown, ['casey: Running late']);
});

test('a message alerts as the member chose once it is read, not as the tab began reading it', async (t) => {
  const { server, hold, shown, sees, start } = tab(t, 'user_rechose', true);
  server.waiting = [waits('jordan', 'm1')];
  start();
  await sees('m1');
  // Half a minute on, it reads what waits again; meanwhile the member turns notifications on.
  const asked = hold(waitingUrl);
  t.mock.timers.tick(30_000);
  const give = await asked;
  dataCache.put(settingsRead, chose(true));
  give({ revision: 0, count: 2, since: [], latest: [...server.waiting, waits('casey', 'm2')] });
  await sees('m2');
  assert.deepEqual(shown, ['casey: Running late']);
});
