import { api, useCachedData, useData } from '../../shell/frontend/runtime/api.js';
import type {
  AuditPage,
  DspBetaTesters,
  DspFeatureReport,
  DspFeatures,
  DspVisibility,
  MailMessage,
  PlatformHealth,
  Visibility,
} from './index.js';
import type { DspProfile, DspSummary } from '../../accounts/api/index.js';
import type { Feature } from '../../tenancy/api/index.js';
import type { Job } from '../../collection/api/index.js';

// The platform owner's endpoints, as its dashboard calls them: DSPs, Diagnostics and the
// audit log.

export const usePlatformDsps = (poll = 0) =>
  useCachedData<DspSummary[]>('/api/platform/dsps', poll);
const dspFeatures = (dspId: string) => `/api/platform/dsps/${dspId}/features`;
export const useDspFeatures = (dspId: string) =>
  useCachedData<DspFeatureReport>(dspFeatures(dspId));
/** Switches one feature and whatever depends on it; the answer lists every switch. */
export const setDspFeature = (dspId: string, feature: Feature, enabled: boolean) =>
  api<DspFeatures>(dspFeatures(dspId), { feature, enabled });
/** Shows a feature to the DSP's members, puts it in beta there, or hides it from them; it keeps
 * running either way. */
export const setDspVisibility = (dspId: string, feature: Feature, visibility: Visibility) =>
  api<DspVisibility>(`${dspFeatures(dspId)}/visibility`, { feature, visibility });
const betaTesters = (dspId: string) => `/api/platform/dsps/${dspId}/beta-testers`;
/** The DSP's people, its beta testers among them. */
export const useDspBetaTesters = (dspId: string) => useData<DspBetaTesters>(betaTesters(dspId));
export const addBetaTesters = (dspId: string, users: string[]) =>
  api<DspBetaTesters>(betaTesters(dspId), { users });
export const removeBetaTester = (dspId: string, user: string) =>
  api<DspBetaTesters>(`${betaTesters(dspId)}/${user}/remove`, {});
/** Hides the AI Support button from the DSP's members, or shows it to them again. */
export const showDspAiSupport = (dspId: string, shown: boolean) =>
  api<DspProfile>(`/api/platform/dsps/${dspId}/ai-support`, { shown });
export const usePlatformMail = (poll = 0) =>
  useCachedData<MailMessage[]>('/api/platform/mail', poll);
/** Gives a failed message a fresh set of attempts, or drops it. */
export const retryMail = (id: string) => api(`/api/platform/mail/${id}/retry`, {});
export const discardMail = (id: string) => api(`/api/platform/mail/${id}/discard`, {});
export const usePlatformJobs = (poll = 0) => useCachedData<Job[]>('/api/platform/jobs', poll);
export const usePlatformHealth = (poll = 0) =>
  useData<PlatformHealth>('/api/platform/health', poll);
const audit = '/api/platform/audit';
export const useAuditPage = (query: URLSearchParams, limit: number) =>
  useData<AuditPage>(`${audit}?${query}&limit=${limit}`, 10000);
export const exportAudit = (query: URLSearchParams) =>
  api<AuditPage>(`${audit}/export`, Object.fromEntries(query));
