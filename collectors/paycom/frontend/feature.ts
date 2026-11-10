import { createElement, lazy } from 'react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

const load = () => import('./index.js');
const PaycomCard = lazy(() => load().then((module) => ({ default: module.PaycomCard })));
// Paycom's connection keeps the address it had before there were others.
const read = '/api/dsp/connections';

export const feature: FrontendFeature = {
  name: 'paycom',
  connectionCard: {
    provider: 'paycom',
    read,
    load,
    render: (context) => createElement(PaycomCard, { ...context, read }),
  },
  platformSlots: () => import('./platform-slots.js'),
  // Core's codes, as they read since Paycom was the only connection.
  errors: {
    connection_required: 'Connect Paycom before starting a collection.',
    verification_incomplete:
      'Paycom still needs verification. Complete the CAPTCHA, then press Submit again.',
    paycom_api_refused: 'Paycom refused the SID and Token.',
    paycom_api_function_off: 'A function Dispatch reads through is turned off in Paycom.',
    paycom_api_unreadable: "Paycom's answer couldn't be read.",
    paycom_api_unavailable: "Connecting through Paycom's API isn't available here yet.",
    paycom_station_missing: 'Paycom has no category named Delivery Station Code.',
    paycom_period_required: 'Enter the first day of a pay period.',
  },
  scheduleIssues: {
    schedule_paycom_required: 'Connect Paycom before enabling this schedule.',
  },
};
