import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { messages } from '../integrations/messages/frontend/integration.js';

// Communications' frontend manifest, loaded up front: it stays small and loads the rest lazily.

export const feature: FrontendFeature = {
  name: 'communications',
  // Its switch's icon on the DSPs page.
  platformSlots: () => import('./platform-slots.js'),
  connectionIntegrations: [messages],
  // Its page waits on changes to the DSP's conversations, and who is typing in them.
  longPolls: ['/api/dsp/communications/updates'],
  // Each of its writes changes its reads, but saying who is typing, which changes none.
  cache: {
    write: (write, url) =>
      write.startsWith('/api/dsp/communications') && !write.endsWith('/typing')
        ? begins(url, '/api/dsp/communications')
        : undefined,
  },
};
