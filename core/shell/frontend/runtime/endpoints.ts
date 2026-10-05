import { performancePolicy } from '../lib/performance-policy.js';
import type { DspView, SessionView } from '../../../accounts/api/index.js';
import { api } from './api.js';

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
