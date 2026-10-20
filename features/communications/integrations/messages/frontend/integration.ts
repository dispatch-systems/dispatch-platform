import { Clock, Mail, MessagesSquare } from 'lucide-react';
import type { ConnectionIntegration } from '../../../../../core/shell/frontend/runtime/slots.js';

// Communications' card under a workspace's Integrations. It's on from the start in every
// workspace; switched off there, Slack answers what's written to the app itself.

/** Its ID, as the backend declares it. */
export const MESSAGES = 'communications.messages';

export const messages: ConnectionIntegration = {
  connection: 'slack',
  id: MESSAGES,
  label: 'Communications',
  icon: MessagesSquare,
  description: 'Messages people send Dispatch in Slack show on the Communications page.',
  steps: ['People message Dispatch in Slack', 'Your team answers on the Communications page'],
  sees: [
    { icon: MessagesSquare, text: 'Messages people send Dispatch, with their photos and files' },
    { icon: Mail, text: "Members' email addresses, to answer under your team's Slack photos" },
  ],
  turningOn: [
    { text: 'Messages people send Dispatch show on the Communications page' },
    { text: 'Messages from before it was on are never kept' },
  ],
  turningOff: [
    { text: 'People who message Dispatch are told DSP Messaging is unavailable' },
    { text: 'Messages already received stay' },
    { icon: Clock, text: "Messages sent while it's off aren't kept" },
  ],
  order: 30,
};
