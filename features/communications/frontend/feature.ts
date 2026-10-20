import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { messages } from '../integrations/messages/frontend/integration.js';

// Communications's frontend manifest, loaded up front: it stays small and loads the rest lazily.

export const feature: FrontendFeature = {
  name: 'communications',
  // Its switch's icon on the DSPs page.
  platformSlots: () => import('./platform-slots.js'),
  connectionIntegrations: [messages],
};
