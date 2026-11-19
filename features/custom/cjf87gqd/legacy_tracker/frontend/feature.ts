import type { FrontendFeature } from '../../../../../core/shell/frontend/runtime/slots.js';
import { useDspCjf87gqdLegacyTrackerSummary } from '../api/client.js';

// Legacy Tracker's frontend manifest, loaded up front: a link on Conduct's tab strip.

export const feature: FrontendFeature = {
  name: 'cjf87gqd_legacy_tracker',
  pageTabs: [
    {
      page: 'conduct',
      id: 'cjf87gqd-legacy-tracker',
      label: 'Legacy Tracker',
      order: 100,
      // The address the DSP set, once read: until then, and with none set, no link is drawn.
      useLink: () => useDspCjf87gqdLegacyTrackerSummary().data?.link ?? undefined,
    },
  ],
  // Its switch's icon on the DSPs page.
  platformSlots: () => import('./platform-slots.js'),
};
