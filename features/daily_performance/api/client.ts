import { useData } from '../../../core/shell/frontend/runtime/api.js';
import type { DailyPerformanceSummary } from './generated/DailyPerformanceSummary.js';

// Typed read for future screens; this feature does not register a page.

export const dailyPerformanceSummaryUrl = '/api/dsp/daily-performance';
/** What Daily Performance holds for the DSP in view. */
export const useDailyPerformanceSummary = () =>
  useData<DailyPerformanceSummary>(dailyPerformanceSummaryUrl);
