import { navigate } from '../../../../core/shell/frontend/runtime/navigation.js';

// The browser's notification of a new message. Where Dispatch's service worker serves the page,
// the page shows it through the worker, as pushes are (`push.ts`), under the message's tag, so a
// push of the same message takes its place rather than showing twice, and clicking it opens it as
// a push's does; else the page shows it itself. A phone's browser shows one only from a service
// worker (Android) or a Home Screen app (iPhone): there only pushes show them.

/** A browser's hint of its device, where it gives one (Chromium's). */
type Hinted = Navigator & { userAgentData?: { mobile: boolean } };

/** Whether this browser can show a notification from the page. */
export const canNotify = () =>
  typeof Notification === 'function' && !(navigator as Hinted).userAgentData?.mobile;

/** Whether this browser allows Dispatch's notifications. */
export const allowed = () =>
  typeof Notification === 'function' && Notification.permission === 'granted';

/** Whether the member may be notified from the page in this browser: it can, and they've allowed it. */
export const notifying = () => canNotify() && allowed();

/**
 * Shows a notification titled `title` saying `body`, of the message `tag`; clicking it brings
 * Dispatch to the front and opens what it's of, at `opens`, if anything.
 */
export function notify(title: string, body: string, tag: string, opens?: string) {
  if (!('serviceWorker' in navigator)) return shown(title, body, tag, opens);
  void navigator.serviceWorker
    .getRegistration('/')
    .then((worker) =>
      worker
        ? worker.showNotification(title, {
            body,
            tag,
            data: { url: opens && new URL(opens, location.href).href },
          })
        : shown(title, body, tag, opens),
    )
    .catch(() => {
      // Not shown, as from a browser that shows them only from a service worker after all.
    });
}

/** Shows a notification from the page itself. */
function shown(title: string, body: string, tag: string, opens?: string) {
  try {
    const notice = new Notification(title, { body, tag });
    notice.onclick = () => {
      window.focus();
      if (opens) navigate(opens);
      notice.close();
    };
  } catch {
    // A browser that shows them only from a service worker after all.
  }
}
