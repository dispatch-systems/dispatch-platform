import { useEffect } from 'react';
import { csrf } from './api.js';
import { recoverAccess } from './access-recovery.js';
import { performancePolicy } from '../lib/performance-policy.js';
import { onActivity } from './activity.js';
import { randomId } from '../lib/random-id.js';

const BEAT = 30_000;
const IDLE = 120_000;
type State = 'active' | 'idle' | 'gone';

let leave: (() => Promise<void>) | undefined;
/** Signing out ends the session, so the dashboard has to report leaving first. */
export const leavePresence = () => leave?.() ?? Promise.resolve();

/** Tells the DSP's team whether this member is using the dashboard. `token` is the open DSP view. */
export function usePresence(token: string | undefined) {
  useEffect(() => {
    if (!token) return;
    // Each view reports as its own tab, so a closing view never clears the one replacing it.
    const tab = randomId();
    let lastActivity = performance.now();
    let sent: State | undefined;
    let active = true;
    const send = (state: State) => {
      if (!active && state !== 'gone') return Promise.resolve();
      sent = state;
      // Presence is best effort: a missed beat only delays the next status.
      return fetch('/api/dsp/presence', {
        method: 'POST',
        credentials: 'same-origin',
        keepalive: true,
        signal: AbortSignal.timeout(performancePolicy.readTimeoutMs),
        headers: {
          'Content-Type': 'application/json',
          'X-CSRF-Token': csrf,
          'X-Dispatch-View': token,
        },
        body: JSON.stringify({ tab, state }),
      }).then(
        async (response) => {
          if (!active || state === 'gone' || ![401, 409].includes(response.status)) return;
          const value = await response.json().catch(() => null);
          if (!active || (response.status !== 401 && value?.error !== 'dsp_view_expired')) return;
          // Stop the rejected view before recovery can install its replacement. Late
          // replies from a view that already closed cannot refresh a newer admission.
          active = false;
          clearInterval(timer);
          recoverAccess('/api/dsp/presence', response.status, value?.error);
        },
        () => undefined,
      );
    };
    const current = (): State => (performance.now() - lastActivity < IDLE ? 'active' : 'idle');
    const activity = () => {
      lastActivity = performance.now();
      if (sent !== 'active') void send('active');
    };
    const unwatch = onActivity(activity);
    // Returning to the dashboard counts as using it; a background tab goes idle on its own.
    const visible = () => {
      if (!document.hidden) activity();
    };
    document.addEventListener('visibilitychange', visible);
    const gone = () => void send('gone');
    // A page restored from the back/forward cache reported itself gone when it was hidden.
    const shown = (event: PageTransitionEvent) => {
      if (event.persisted) activity();
    };
    window.addEventListener('pagehide', gone);
    window.addEventListener('pageshow', shown);
    // The idle check runs more often than the beat so Idle appears close to the two-minute mark.
    let beatAt = performance.now();
    const timer = window.setInterval(() => {
      const state = current();
      if (state === sent && performance.now() - beatAt < BEAT) return;
      beatAt = performance.now();
      void send(state);
    }, 5000);
    void send('active');
    leave = () => send('gone');
    return () => {
      active = false;
      leave = undefined;
      clearInterval(timer);
      unwatch();
      document.removeEventListener('visibilitychange', visible);
      window.removeEventListener('pagehide', gone);
      window.removeEventListener('pageshow', shown);
      if (sent !== 'gone') void send('gone');
    };
  }, [token]);
}
