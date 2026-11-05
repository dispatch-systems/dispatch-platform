import { getPushKey, keepPushing, stopPushing } from '../../api/client.js';

// Telling the member of new messages with Dispatch closed, or on a phone (Web Push): while they
// chose Notifications and this browser allows them, it's subscribed with its push service under
// the server's key, and the subscription kept as theirs; otherwise, or once the server pushes
// nothing, it's let go of. Dispatch's service worker (`app/frontend/public/service-worker.js`)
// shows what's pushed. Each tab follows what the member chose as it opens and as they change it,
// so a browser whose subscription changed, or that the server let go of, is kept again.

/** Dispatch's service worker, at the address's root, so it serves every page. */
const WORKER = '/service-worker.js';

/** Whether this browser can be pushed to: through a service worker, with the page closed. */
export const canPush = () =>
  typeof Notification === 'function' && 'serviceWorker' in navigator && 'PushManager' in window;

/**
 * What becomes of this browser's subscription, made under the key `subscribed` (empty for one
 * made under no key it says), or none: made under `key`, the server's, in place of any other; kept
 * as it is, told to the server again; or let go of.
 */
export function step(wanted: boolean, key: string | null, subscribed: string | undefined) {
  if (!wanted || !key) return subscribed === undefined ? 'none' : 'remove';
  return subscribed === key ? 'keep' : 'subscribe';
}

/** Has this browser pushed to as `wanted`, where it can be. */
export async function followPush(wanted: boolean) {
  if (!canPush()) return;
  const { key } = await getPushKey();
  const worker =
    wanted && key
      ? await navigator.serviceWorker.register(WORKER)
      : await navigator.serviceWorker.getRegistration('/');
  const current = (await worker?.pushManager.getSubscription()) ?? null;
  const under = current?.options.applicationServerKey;
  switch (step(wanted, key, current ? (under ? encode(under) : '') : undefined)) {
    case 'remove':
      await stopPushing(current!.endpoint);
      await current!.unsubscribe();
      return;
    case 'keep':
      return kept(current!);
    case 'subscribe':
      if (current) {
        await stopPushing(current.endpoint);
        await current.unsubscribe();
      }
      return kept(
        await worker!.pushManager.subscribe({
          userVisibleOnly: true,
          applicationServerKey: decode(key!),
        }),
      );
  }
}

/** Tells the server of `subscription`, as the member's. */
function kept(subscription: PushSubscription) {
  const { endpoint, keys } = subscription.toJSON();
  return keepPushing(endpoint!, { p256dh: keys!.p256dh!, auth: keys!.auth! });
}

/** Bytes in base64url, unpadded, as the server writes its key. */
const encode = (bytes: ArrayBuffer) =>
  btoa(String.fromCharCode(...new Uint8Array(bytes)))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
const decode = (text: string) =>
  Uint8Array.from(atob(text.replace(/-/g, '+').replace(/_/g, '/')), (char) => char.charCodeAt(0));
