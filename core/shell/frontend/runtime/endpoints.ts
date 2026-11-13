import { performancePolicy } from '../lib/performance-policy.js';
import type { DspView, SessionView, SidebarArrangement } from '../../../accounts/api/index.js';
import { api, view, viewing } from './api.js';

// The session's and the DSP view's endpoints. Every owner's own are in its api/client.ts, each
// address written once next to the type it answers with; a few pages still call `api` and
// `useData` directly.

export const getSession = () => api<SessionView>('/api/session');
export const openDsp = (dspId: string, roleId?: string) =>
  api<DspView>(
    '/api/session/dsp',
    roleId ? { dspId, roleId } : { dspId },
    AbortSignal.timeout(performancePolicy.readTimeoutMs),
  );
/**
 * A save of how the member arranged the sidebar of the DSP open now, to send in its turn: it
 * sends nothing once that view is no longer the one open, as for a DSP left meanwhile.
 */
export function sidebarSave(arrangement: SidebarArrangement) {
  const of = viewing(view);
  return async () => {
    if (of === viewing(view)) await api('/api/dsp/sidebar', arrangement);
  };
}
