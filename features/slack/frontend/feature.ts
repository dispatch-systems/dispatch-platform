import { createElement, lazy } from 'react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Slack's frontend manifest, loaded up front: it stays small and loads its card lazily. Its
// error codes are worded in its API client, which loads with the card: only the card makes the
// calls that raise them.

const loadCards = () => import('./settings.js');
const SlackCard = lazy(() => loadCards().then((module) => ({ default: module.SlackCard })));

export const feature: FrontendFeature = {
  name: 'slack',
  // Its workspaces are among the DSP's own accounts.
  connectionPieces: [
    {
      id: 'slack',
      section: 'dsp',
      order: 20,
      visible: (view) => can(view, 'slack.manage'),
      reads: ['/api/dsp/slack/account'],
      load: loadCards,
      render: ({ view }) => view && createElement(SlackCard, { view }),
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
};
