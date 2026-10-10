import { api, useCachedData, useData } from '../../shell/frontend/runtime/api.js';
import type {
  AuditPage,
  DspFeatureReport,
  DspFeatures,
  DspHidden,
  MailMessage,
  PlatformHealth,
} from './index.js';
import type { DspSummary } from '../../accounts/api/index.js';
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
/** Hides a feature from the DSP's members, or shows it again; it keeps running either way. */
export const showDspFeature = (dspId: string, feature: Feature, shown: boolean) =>
  api<DspHidden>(`${dspFeatures(dspId)}/shown`, { feature, shown });
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
