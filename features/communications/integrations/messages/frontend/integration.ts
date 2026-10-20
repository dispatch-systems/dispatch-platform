import { MessagesSquare } from 'lucide-react';
import type { ConnectionIntegration } from '../../../../../core/shell/frontend/runtime/slots.js';

// Communications's card under a workspace's Integrations.

/** Its ID, as the backend declares it. */
export const MESSAGES = 'communications.messages';

export const messages: ConnectionIntegration = {
  connection: 'slack',
  id: MESSAGES,
  label: 'Communications',
  icon: MessagesSquare,
  // TODO(scaffold): what it does in a sentence or two, how it's used step by step, what
  // Dispatch sees through Slack while it's on, and what switching it on and off means.
  description: '',
  steps: [],
  sees: [],
  turningOn: [],
  turningOff: [],
  order: 30,
};
