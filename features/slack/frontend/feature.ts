import { createElement, lazy } from 'react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Slack's frontend manifest, loaded up front: it stays small and loads its cards lazily. Its
// error codes are worded in its API client, which loads with the cards: only they and New make
// the calls that raise them.

const loadCards = () => import('./settings.js');
const SlackCards = lazy(() => loadCards().then((module) => ({ default: module.SlackCards })));
const NewSlack = lazy(() => loadCards().then((module) => ({ default: module.NewSlack })));
const SlackLogo = lazy(() => loadCards().then((module) => ({ default: module.SlackLogo })));

export const feature: FrontendFeature = {
  name: 'slack',
  // Its workspaces are among the DSP's own accounts, each on a card of its own; New adds one.
  connectionPieces: [
    {
      id: 'slack',
      section: 'dsp',
      order: 20,
      reads: ['/api/dsp/slack/account'],
      load: loadCards,
      render: ({ view }) => view && createElement(SlackCards, { view }),
      adds: {
        label: 'Slack',
        icon: () => createElement(SlackLogo, { size: 16 }),
        render: ({ close }) => createElement(NewSlack, { close }),
      },
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
};
