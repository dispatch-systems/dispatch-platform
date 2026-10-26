import { createElement, lazy } from 'react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { phonesUrl } from '../api/client.js';

// Phones' frontend manifest, loaded up front: it stays small and loads the rest lazily.

const loadCard = () => import('./connection/PhonesCard.js');
const PhonesCard = lazy(() => loadCard().then((module) => ({ default: module.PhonesCard })));

export const feature: FrontendFeature = {
  name: 'phones',
  // The DSP's phones, on Settings' Connections, for those who manage them or all its connections.
  connectionPieces: [
    {
      id: 'phones',
      section: 'dsp',
      order: 100,
      visible: (view) => can(view, 'connections.manage') || can(view, 'phones.manage'),
      // Managing the DSP's phones alone shows it, as managing all its connections does.
      others: (view) => can(view, 'phones.manage'),
      reads: [phonesUrl],
      load: loadCard,
      render: ({ view }) => view && createElement(PhonesCard, { view }),
    },
  ],
  // Its switch's icon, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
  // What `texting` refuses with, on whichever page uses it, as its contract lists them.
  errors: {
    phones_none: 'Pair a phone first.',
    phones_not_found: "That phone isn't paired anymore. Refresh the page.",
    phones_off: 'Phones is off for this DSP.',
    phones_file_not_found: "That picture isn't there anymore.",
    phones_text_not_found: "That text can't be sent again.",
  },
};
