// Dispatch's service worker, at its address's root so it serves every page there: it shows what
// the server pushes as a notification, and clicking one brings Dispatch to the front at what it's
// of. Nothing else: it keeps nothing and answers no requests. A push is JSON, `title`, `body`,
// `url` and `tag`; one arriving while a window of Dispatch here is focused isn't shown, as that
// page alerts itself, and one with the tag of a notification shown takes its place.

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
      if (windows.some((window) => window.focused && window.visibilityState === 'visible')) return;
      await self.registration.showNotification(push.title, {
        body: push.body,
        tag: push.tag,
        data: { url: push.url },
      });
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
