import { createElement, lazy } from 'react';
import { BriefcaseBusiness } from 'lucide-react';
import { lineSlots, type TextingLine } from '../../../../communications/frontend/lines.js';
import { can } from '../../../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../../../core/shell/frontend/runtime/slots.js';
import { hrLinePhonesUrl, hrLineTexting } from '../api/client.js';

declare module '../../../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    'cjf87gqd-hr-line': true;
  }
}

// HR Line's frontend manifest, loaded up front: it stays small and loads the rest lazily.

const loadCard = () => import('./connection/HrLineCard.js');
const HrLineCard = lazy(() => loadCard().then((module) => ({ default: module.HrLineCard })));

/** Its line of the DSP's phones, as Dispatch Phone's page, count, alerts, banner and Display
 * section draw it for HR Line alone. */
const line: TextingLine = {
  id: 'cjf87gqd_hr_line',
  name: 'HR Line',
  page: 'cjf87gqd-hr-line',
  permission: 'cjf87gqd_hr_line.use',
  texting: hrLineTexting,
  display: 'cjf87gqd_hr_line.display',
  phones: 'cjf87gqd_hr_line.phones',
  phone: 'HR Line phone',
};
const { page, ...slots } = lineSlots(line);

export const feature: FrontendFeature = {
  name: 'cjf87gqd_hr_line',
  // Its page, for those who use HR Line, with how many of its conversations wait.
  routes: [
    {
      id: 'cjf87gqd-hr-line',
      scope: 'dsp',
      label: 'HR Line',
      icon: BriefcaseBusiness,
      feature: 'cjf87gqd_hr_line',
      ...page,
    },
  ],
  // How its page looks to each member, on Settings' Display page; alerting them to its new texts
  // on every page, and a banner atop every page while one of its phones is down; and how the
  // dashboard waits on and caches its reads.
  ...slots,
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
