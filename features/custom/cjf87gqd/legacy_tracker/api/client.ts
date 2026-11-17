import { useData } from '../../../../../core/shell/frontend/runtime/data-reads.js';
import type { DspCjf87gqdLegacyTrackerSummary } from './generated/DspCjf87gqdLegacyTrackerSummary.js';

// Legacy Tracker's endpoints, as its screens call them.

export const dspCjf87gqdLegacyTrackerSummaryUrl = '/api/dsp/cjf87gqd-legacy-tracker';
/** The address the DSP's link opens. */
export const useDspCjf87gqdLegacyTrackerSummary = () =>
  useData<DspCjf87gqdLegacyTrackerSummary>(dspCjf87gqdLegacyTrackerSummaryUrl);
