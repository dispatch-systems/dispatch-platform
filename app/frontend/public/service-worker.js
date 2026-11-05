// Dispatch's service worker, at its address's root so it serves every page there: it shows what
// the server pushes as a notification, and clicking one brings Dispatch to the front at what it's
// of. Nothing else: it keeps nothing and answers no requests. A push is JSON, `title`, `body`,
// `url` and `tag`, with what it's of for a page to judge, and one with the tag of a notification
// shown takes its place. Browsers ask that every push shows one, penalising a push that shows
// none: one a window of Dispatch in front says it alerts of itself is shown silently and closed
// at once, or left to that page's own notification of it; asked, a window that doesn't say so in
// a moment, as one of another DSP's or that alerts of nothing, leaves it shown.

/** How long a window in front has to say it alerts of a push itself. */
const ASK_MS = 1000;

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()));

/** Whether a window of Dispatch in front, of `windows`, says it alerts of `push` itself. */
async function alertedInFront(windows, push) {
  const asked = windows
    .filter((window) => window.focused && window.visibilityState === 'visible')
    .map(
      (window) =>
        new Promise((answered) => {
          const channel = new MessageChannel();
          const answer = (alerts) => {
            channel.port1.close();
            answered(alerts);
          };
          channel.port1.onmessage = (event) => answer(event.data?.alerts === true);
          setTimeout(() => answer(false), ASK_MS);
          window.postMessage({ push }, [channel.port2]);
        }),
    );
  return (await Promise.all(asked)).includes(true);
}

self.addEventListener('push', (event) => {
  let push;
  try {
    push = event.data?.json();
  } catch {
    return;
  }
  if (!push) return;
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
      const alerted = await alertedInFront(windows, push);
      // The page in front showed its own already: that one shows this push.
      if (alerted && (await self.registration.getNotifications({ tag: push.tag })).length) return;
      await self.registration.showNotification(push.title, {
        body: push.body,
        tag: push.tag,
        silent: alerted,
        data: { url: push.url },
      });
      if (!alerted) return;
      for (const shown of await self.registration.getNotifications({ tag: push.tag }))
        shown.close();
    })(),
  );
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const url = event.notification.data?.url;
  if (!url) return;
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
      const open = windows.find((window) => 'focus' in window);
      if (!open) return self.clients.openWindow(url);
      await open.focus();
      // A window this worker doesn't serve yet can't be moved: another opens.
      await open.navigate(url).catch(() => self.clients.openWindow(url));
    })(),
  );
});
