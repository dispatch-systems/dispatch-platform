import type { Job, ScheduleInput, SchedulePreview } from '../../../core/collection/api/index.js';
import { wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { DvicInspections } from './index.js';

// DVIC's endpoints, as its page and its settings call them. Only its own screens raise this,
// so its words load with them; a station's absence a schedule meets too, so the manifest
// words that.
const api = wordedApi({ dvic_week_not_available: 'That report week is not available yet.' });

/** Where its schedules are served, as core's schedule calls take it. */
export const schedules = '/api/dsp/dvic/schedules';
/** When a schedule with this timing would run next. */
export const previewSchedule = (
  timing: Pick<ScheduleInput, 'cadence' | 'intervalMinutes' | 'localTime'> & {
    scheduleId?: string;
  },
  signal: AbortSignal,
) => api<SchedulePreview>(`${schedules}/preview`, timing, signal);
/** Starts collecting the latest week, once for `requestId`. */
export const collectDvic = (requestId: string) => api<Job>('/api/dsp/dvic/collect', { requestId });
export const cancelDvicJob = (id: string) => api(`/api/dsp/dvic/jobs/${id}/cancel`, {});
/** One page of a week's inspections, from its address. */
export const inspectionPage = (url: string, signal: AbortSignal) =>
  api<DvicInspections>(url, undefined, signal);
