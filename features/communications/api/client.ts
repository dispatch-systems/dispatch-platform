import { useData } from '../../../core/shell/frontend/runtime/api.js';
import type { CommunicationsSummary } from './generated/CommunicationsSummary.js';

// Communications's endpoints, as its screens call them.

export const communicationsSummaryUrl = '/api/dsp/communications';
/** What Communications holds for the DSP in view. */
export const useCommunicationsSummary = () =>
  useData<CommunicationsSummary>(communicationsSummaryUrl);
