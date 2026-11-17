import { createElement, lazy } from 'react';
import { can } from '../../../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../../../core/shell/frontend/runtime/slots.js';
import { hrLinePhonesUrl } from '../api/client.js';

// HR Line's frontend manifest, loaded up front: it stays small and loads the rest lazily.

const loadCard = () => import('./connection/HrLineCard.js');
const HrLineCard = lazy(() => loadCard().then((module) => ({ default: module.HrLineCard })));

export const feature: FrontendFeature = {
  name: 'cjf87gqd_hr_line',
  // Its phones, on Settings' Connections, beside Dispatch Phone's, for those who manage them or
  // all the DSP's connections.
  connectionPieces: [
    {
      id: 'cjf87gqd-hr-line',
      section: 'dsp',
      order: 101,
      visible: (view) => can(view, 'connections.manage') || can(view, 'cjf87gqd_hr_line.phones'),
      // Managing HR Line's phones alone shows it, as managing all the DSP's connections does.
      others: (view) => can(view, 'cjf87gqd_hr_line.phones'),
      reads: [hrLinePhonesUrl],
      load: loadCard,
      render: ({ view }) => view && createElement(HrLineCard, { view }),
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
};
