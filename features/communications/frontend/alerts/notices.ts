// The browser's notification of a new message. A phone's browser shows one only from a service
// worker (Android) or a Home Screen app (iPhone), which Dispatch has neither of, so there it
// shows none.

/** A browser's hint of its device, where it gives one (Chromium's). */
type Hinted = Navigator & { userAgentData?: { mobile: boolean } };

/** Whether this browser can show a notification from the page. */
export const canNotify = () =>
  typeof Notification === 'function' && !(navigator as Hinted).userAgentData?.mobile;

/** Whether the member may be notified in this browser: it can, and they've allowed it. */
export const notifying = () => canNotify() && Notification.permission === 'granted';

/**
 * Shows a notification titled `title` saying `body`, in place of an earlier one under `tag`;
 * clicking it brings Dispatch to the front and `opens` what it's of.
 */
export function notify(title: string, body: string, tag: string, opens: () => void) {
  try {
    const notice = new Notification(title, { body, tag });
    notice.onclick = () => {
      window.focus();
      opens();
      notice.close();
    };
  } catch {
    // A browser that shows them only from a service worker after all.
  }
}
