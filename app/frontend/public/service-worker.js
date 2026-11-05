// Dispatch's service worker, at its address's root so it serves every page there: it shows what
// the server pushes as a notification, and clicking one brings Dispatch to the front at what it's
// of. Nothing else: it keeps nothing and answers no requests. A push is JSON, `title`, `body`,
// `url` and `tag`, and one with the tag of a notification shown takes its place. Browsers ask that
// every push shows one, penalising a push that shows none: one arriving while a window of Dispatch
// here is focused, which alerts itself, is shown silently and closed at once, unless that page
// showed its own of it already.

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()));

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
      const watched = windows.some(
        (window) => window.focused && window.visibilityState === 'visible',
      );
      // The page in front showed its own already: that one shows this push.
      if (watched && (await self.registration.getNotifications({ tag: push.tag })).length) return;
      await self.registration.showNotification(push.title, {
        body: push.body,
        tag: push.tag,
        silent: watched,
        data: { url: push.url },
      });
      if (!watched) return;
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
