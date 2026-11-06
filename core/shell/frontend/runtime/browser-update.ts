import { performancePolicy } from '../lib/performance-policy.js';
import { useEffect } from 'react';
import { onActivity } from './activity.js';
import { saveNavigationState, takeRestoredScrollPosition } from './navigation-state.js';

let writes = 0;
export function beginBrowserWrite() {
  writes++;
  return () => {
    writes--;
  };
}

export function useBrowserUpdate(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return;
    const build = document.querySelector<HTMLMetaElement>('meta[name="dispatch-build"]')?.content;
    if (!build) return; // Vite development sessions use HMR.
    let lastActivity = performance.now();
    let candidate: string | undefined;
    let checkedAt = 0;
    let checking = false;
    let stopped = false;
    let reloading = false;
    const controller = new AbortController();
    const activity = () => {
      lastActivity = performance.now();
    };
    const unwatch = onActivity(activity);
    const blocked = () =>
      document.hidden ||
      writes > 0 ||
      Boolean(
        document.querySelector(
          'form, [role="dialog"], [aria-busy="true"], [contenteditable="true"]',
        ),
      );
    const check = async () => {
      if (checking || stopped || document.hidden) return;
      checking = true;
      try {
        const response = await fetch('/api/browser-update', {
          cache: 'no-store',
          signal: AbortSignal.any([controller.signal, AbortSignal.timeout(5000)]),
        });
        if (!response.ok) throw new Error('Update check unavailable');
        const next = (await response.json()) as { ready?: unknown; build?: unknown };
        if (stopped) return;
        candidate =
          next.ready === true &&
          typeof next.build === 'string' &&
          /^[a-f0-9]{64}$/.test(next.build) &&
          next.build !== build
            ? next.build
            : undefined;
        checkedAt = performance.now();
      } catch {
        candidate = undefined; // An unreachable or restarting server is never a reload signal.
      } finally {
        checking = false;
      }
    };
    const tick = () => {
      if (!candidate || reloading || blocked() || performance.now() - lastActivity < 2000) return;
      if (performance.now() - checkedAt > 1500) {
        void check();
        return;
      }
      try {
        const guardKey = 'dispatch:browser-update:last';
        const last = JSON.parse(sessionStorage.getItem(guardKey) || 'null') as {
          build?: string;
          at?: number;
        } | null;
        if (last?.build === candidate && Date.now() - (last.at ?? 0) < 300_000) return;
        sessionStorage.setItem(guardKey, JSON.stringify({ build: candidate, at: Date.now() }));
        saveNavigationState();
      } catch {
        // Without storage we cannot protect navigation state or prevent reload loops.
        return;
      }
      reloading = true;
      location.reload();
    };
    const visible = () => {
      activity();
      if (!document.hidden) void check();
    };
    document.addEventListener('visibilitychange', visible);
    const polling = window.setInterval(() => void check(), performancePolicy.browserUpdatePollMs);
    const idle = window.setInterval(tick, 500);
    void check();
    return () => {
      stopped = true;
      controller.abort();
      clearInterval(polling);
      clearInterval(idle);
      unwatch();
      document.removeEventListener('visibilitychange', visible);
    };
  }, [enabled]);
  useEffect(() => {
    if (!enabled) return;
    const saved = takeRestoredScrollPosition();
    if (!saved) return;
    const deadline = performance.now() + 10_000;
    const restore = () => {
      if (location.hash !== saved.hash || performance.now() > deadline) {
        clearInterval(timer);
        return;
      }
      if (document.documentElement.scrollHeight >= saved.y + innerHeight) {
        window.scrollTo(saved.x, saved.y);
        clearInterval(timer);
      }
    };
    const timer = window.setInterval(restore, 100);
    const cancel = () => clearInterval(timer);
    const cancelEvents = ['pointerdown', 'keydown', 'wheel', 'touchstart'];
    for (const event of cancelEvents)
      window.addEventListener(event, cancel, { once: true, passive: true });
    return () => {
      clearInterval(timer);
      for (const event of cancelEvents) window.removeEventListener(event, cancel);
    };
  }, [enabled]);
}
