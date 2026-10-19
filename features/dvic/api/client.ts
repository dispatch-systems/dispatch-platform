import type { Job } from '../../../core/collection/api/index.js';
import { wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { DvicSummary, DvicInspectionPage, DvicInspectionDetail } from './index.js';
import { useCachedData } from '../../../core/shell/frontend/runtime/api.js';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';

// DVIC's endpoints, as its page calls them. Only its own screens raise this, so its words load
// with them; a station's absence a schedule meets too, so the manifest words that.
const api = wordedApi({ dvic_week_not_available: 'That report week is not available yet.' });

/** Starts collecting the latest weeks and the week of `viewing`, once for `requestId`. */
export const collectDvic = (requestId: string, viewing: string) =>
  api<Job>('/api/dsp/dvic/collect', { requestId, viewing });
export const cancelDvicJob = (id: string) => api(`/api/dsp/dvic/jobs/${id}/cancel`, {});
/** Counts cover the whole selection; the driver page carries at most seven cells each. */
export const summaryUrl = (from: string, to: string, query: string, vehicles: string, page = 0) =>
  `/api/dsp/dvic/summary?${new URLSearchParams({ from, to, q: query, vehicles, offset: String(page * 25) }).toString()}`;
export const useSummary = (url: string, publication: string) =>
  useCachedData<DvicSummary>(url, performancePolicy.recoveryPollMs, publication);
export const useDayInspections = (
  date: string,
  query: string,
  vehicles: string,
  page: number,
  publication: string,
) =>
  useCachedData<DvicInspectionPage>(
    `/api/dsp/dvic/day?${new URLSearchParams({ date, q: query, vehicles, offset: String(page * 100) }).toString()}`,
    performancePolicy.recoveryPollMs,
    publication,
  );
export const useInspection = (id: string, from: string, to: string, publication: string) =>
  useCachedData<DvicInspectionDetail>(
    `/api/dsp/dvic/inspection?${new URLSearchParams({ id, from, to }).toString()}`,
    performancePolicy.recoveryPollMs,
    publication,
  );
