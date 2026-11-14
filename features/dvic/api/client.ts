import { useCallback } from 'react';
import type { Job } from '../../../core/collection/api/index.js';
import { wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { DvicSummary, DvicInspectionPage, DvicInspectionDetail, DvicStatus } from './index.js';
import { useCachedData } from '../../../core/shell/frontend/runtime/data-reads.js';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';

// DVIC's endpoints, as its page calls them. Only its own screens raise this, so its words load
// with them; a station's absence a schedule meets too, so the manifest words that.
const api = wordedApi({ dvic_week_not_available: 'That report week is not available yet.' });

/** Starts collecting the latest weeks and the week of `viewing`, once for `requestId`. */
export const collectDvic = (requestId: string, viewing: string) =>
  api<Job>('/api/dsp/dvic/collect', { requestId, viewing });
export const cancelDvicJob = (id: string) => api(`/api/dsp/dvic/jobs/${id}/cancel`, {});
export const useDvicStatus = (poll: number) =>
  useCachedData<DvicStatus>('/api/dsp/dvic/status', poll);
/** Counts cover the whole selection; the driver page carries at most seven cells each. */
export const summaryUrl = (from: string, to: string, query: string, vehicles: string, page = 0) =>
  `/api/dsp/dvic/summary?${new URLSearchParams({ from, to, q: query, vehicles, offset: String(page * 25) }).toString()}`;
export const useSummary = (url: string, publication: string) =>
  useCachedData<DvicSummary>(url, performancePolicy.recoveryPollMs, publication);
const dayUrl = (date: string, query: string, vehicles: string, page: number) =>
  `/api/dsp/dvic/day?${new URLSearchParams({ date, q: query, vehicles, offset: String(page * 100) }).toString()}`;
/** A day's short inspections, shortest first, a page of 100. */
export const useDayInspections = (
  date: string,
  query: string,
  vehicles: string,
  page: number,
  publication: string,
) =>
  useCachedData<DvicInspectionPage>(
    dayUrl(date, query, vehicles, page),
    performancePolicy.recoveryPollMs,
    publication,
  );
/** Every short inspection of `date`, shortest first, read a page at a time; none without a day. */
export function useWholeDay(date: string, publication: string) {
  const load = useCallback(
    async (signal: AbortSignal) => {
      const first = await api<DvicInspectionPage>(dayUrl(date, '', '', 0), undefined, signal);
      const inspections = [...first.inspections];
      for (let page = 1; inspections.length < first.total; page++) {
        const next = await api<DvicInspectionPage>(dayUrl(date, '', '', page), undefined, signal);
        if (!next.inspections.length) break;
        inspections.push(...next.inspections);
      }
      return { ...first, inspections };
    },
    [date],
  );
  return useCachedData<DvicInspectionPage>(
    date ? dayUrl(date, '', '', 0) : '',
    performancePolicy.recoveryPollMs,
    publication,
    load,
  );
}
export const useInspection = (id: string, from: string, to: string, publication: string) =>
  useCachedData<DvicInspectionDetail>(
    `/api/dsp/dvic/inspection?${new URLSearchParams({ id, from, to }).toString()}`,
    performancePolicy.recoveryPollMs,
    publication,
  );
