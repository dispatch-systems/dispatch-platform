import type { LineTexting } from '../../api/client.js';
import { DISPATCH_PHONE, keptAt, type TextingLine } from '../line.js';
import type { InFront } from './arrivals.js';

// Telling the member of new messages with Dispatch closed, or on a phone (Web Push): while they
// chose Notifications and this browser allows them, it's subscribed with its push service under
// the server's key, and the subscription kept as theirs at the DSP; otherwise, or once the server
// pushes nothing, the DSP lets go of it. Dispatch's service worker
// (`app/frontend/public/service-worker.js`) shows what's pushed. Each tab follows what the member
// chose as it opens and as they change it, so a browser whose subscription changed, or that the
// server let go of, is kept again.
//
// A browser has one subscription for its address, which every DSP opened there shares, as a
// platform owner opens many at the admin's, and each line of a DSP's phones apart: the browser
// remembers which DSPs' lines keep it, and lets go of it only once none does.

/** Dispatch's service worker, at the address's root, so it serves every page. */
const WORKER = '/service-worker.js';

/** Whether this browser can be pushed to: through a service worker, with the page closed. */
export const canPush = () =>
  typeof Notification === 'function' && 'serviceWorker' in navigator && 'PushManager' in window;

/**
 * What becomes of this browser's subscription for a DSP, made under the key `subscribed` (empty
 * for one made under no key it says), or none, while `others` of the DSPs opened here keep it: made
 * under `key`, the server's, in place of any other; kept as it is, told to the server again; let
 * go of by this DSP alone (`leave`), as others keep it; or let go of.
 */
export function step(
  wanted: boolean,
  key: string | null,
  subscribed: string | undefined,
  others: boolean,
) {
  if (!wanted || !key) {
    if (subscribed === undefined) return 'none';
    return key && others ? 'leave' : 'remove';
  }
  return subscribed === key ? 'keep' : 'subscribe';
}

/** The lock every tab of Dispatch here takes its turn changing the subscription under. */
const LOCK = 'dispatch-communications-push';
/** This tab's changes in turn, where the browser has no Web Locks. */
let turns: Promise<unknown> = Promise.resolve();
/** Does `work` in its turn among every tab's changes, unless `ends` ends it before then. */
function inTurn(ends: AbortSignal, work: () => Promise<void>) {
  if ('locks' in navigator) return navigator.locks.request(LOCK, { signal: ends }, work);
  const next = turns.then(() => {
    ends.throwIfAborted();
    return work();
  });
  turns = next.catch(() => {});
  return next;
}

/**
 * Has this browser pushed to as `wanted` by the DSP `dsp` for `line`, where it can be: in its turn
 * among every tab's changes, reading the subscription and which DSPs' lines keep it as the turn
 * starts. Once `ends` ends it, as the page moved on to another DSP or member, or the member chose
 * again, it changes nothing more, on the server or in the browser.
 */
export function followPush(
  wanted: boolean,
  dsp: string,
  ends: AbortSignal,
  line: TextingLine = DISPATCH_PHONE,
) {
  if (!canPush()) return Promise.resolve();
  const { texting } = line;
  // Each line of the DSP keeps it apart.
  const keeper = keptAt(line, dsp);
  /** What `doing` comes to, unless it ended meanwhile. */
  const still = async <T>(doing: Promise<T>) => {
    const done = await doing;
    ends.throwIfAborted();
    return done;
  };
  return inTurn(ends, async () => {
    const { key } = await still(texting.getPushKey(ends));
    // Subscribed only once the worker is active, as browsers ask.
    const worker = await still(
      wanted && key
        ? navigator.serviceWorker.register(WORKER).then(() => navigator.serviceWorker.ready)
        : navigator.serviceWorker.getRegistration('/'),
    );
    const current = await still(
      worker ? worker.pushManager.getSubscription() : Promise.resolve(null),
    );
    const under = current?.options.applicationServerKey;
    const keeping = current ? keepers(current.endpoint).filter((each) => each !== keeper) : [];
    switch (
      step(wanted, key, current ? (under ? encode(under) : '') : undefined, keeping.length > 0)
    ) {
      case 'leave':
        await still(texting.stopPushing(current!.endpoint, ends));
        return keep(current!.endpoint, keeping);
      case 'remove':
        await still(texting.stopPushing(current!.endpoint, ends));
        await current!.unsubscribe();
        return keep(current!.endpoint, []);
      // Each DSP is remembered as keeping it before the server is told, as an answer cut short
      // may come after the server kept it: one remembered that the server doesn't keep only keeps
      // the browser subscribed.
      case 'keep':
        keep(current!.endpoint, [...keeping, keeper]);
        await still(kept(texting, current!, ends));
        return;
      case 'subscribe': {
        // Under the server's new key, the DSPs that kept the old one keep this once opened again.
        if (current) {
          await still(texting.stopPushing(current.endpoint, ends));
          await still(current.unsubscribe());
        }
        const made = await still(
          worker!.pushManager.subscribe({
            userVisibleOnly: true,
            applicationServerKey: decode(key!),
          }),
        );
        keep(made.endpoint, [keeper]);
        await still(kept(texting, made, ends));
        return;
      }
    }
  });
}

/** Tells the server of `subscription`, as the member's, for the line `texting` calls. */
function kept(texting: LineTexting, subscription: PushSubscription, ends: AbortSignal) {
  const { endpoint, keys } = subscription.toJSON();
  return texting.keepPushing(endpoint!, { p256dh: keys!.p256dh!, auth: keys!.auth! }, ends);
}

/** Where this browser remembers which DSPs keep its subscription, by its endpoint. */
const KEEPERS = 'dispatch-communications-push';
/** The DSPs' lines that keep the subscription at `endpoint`, as this browser remembers them. */
function keepers(endpoint: string): string[] {
  try {
    const kept = JSON.parse(localStorage.getItem(KEEPERS) ?? 'null') as {
      endpoint: string;
      dsps: string[];
    } | null;
    return kept?.endpoint === endpoint ? kept.dsps : [];
  } catch {
    return [];
  }
}
function keep(endpoint: string, dsps: string[]) {
  try {
    localStorage.setItem(KEEPERS, JSON.stringify({ endpoint, dsps }));
  } catch {
    // Not remembered, the next DSP to let go of it lets go of it for every DSP, until reopened.
  }
}

/**
 * A push, as the service worker asks a page in front about it: of a line of the DSP's phones, none
 * for Dispatch Phone's own, as a push before lines were says none.
 */
export type Pushed = {
  dsp?: string;
  line?: string | null;
  conversation?: string;
  group?: string | null;
};

/**
 * Whether a page of the DSP `dsp`'s line `line` (Dispatch Phone's own for none), in front of the
 * member with `inFront` open, alerts of `push` itself, so the service worker shows it quietly: one
 * of its own DSP's and line's, of the conversation or group open, or that it notifies of from the
 * page (`notifies`). Another line's it leaves shown, as its own page alerts of it.
 */
export function alertsInFront(
  push: Pushed,
  dsp: string,
  inFront: InFront,
  notifies: boolean,
  line: string | null = null,
) {
  if (push.dsp !== dsp || (push.line ?? null) !== line) return false;
  const open =
    (push.conversation !== undefined && push.conversation === inFront.conversation) ||
    (Boolean(push.group) && push.group === inFront.group);
  return open || notifies;
}

/**
 * Answers the service worker asking whether this page alerts of a push itself, by `alerts`, until
 * the answer is called; none for a push `alerts` leaves to another line's, as the worker takes
 * the first answer.
 */
export function answerWorker(alerts: (push: Pushed) => boolean | undefined) {
  if (!('serviceWorker' in navigator)) return () => {};
  const asked = (event: MessageEvent) => {
    const push = (event.data as { push?: Pushed } | null)?.push;
    const answer = push && alerts(push);
    if (answer !== undefined) event.ports[0]?.postMessage({ alerts: answer });
  };
  navigator.serviceWorker.addEventListener('message', asked);
  return () => navigator.serviceWorker.removeEventListener('message', asked);
}

/** Bytes in base64url, unpadded, as the server writes its key. */
const encode = (bytes: ArrayBuffer) =>
  btoa(String.fromCharCode(...new Uint8Array(bytes)))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
const decode = (text: string) =>
  Uint8Array.from(atob(text.replace(/-/g, '+').replace(/_/g, '/')), (char) => char.charCodeAt(0));
