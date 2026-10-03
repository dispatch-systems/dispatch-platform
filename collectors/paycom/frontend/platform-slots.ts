import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Paycom puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  auditWording: wording,
  capabilities: { timecards: 'a timecard source' },
  collections: [
    {
      kind: 'paycom.collect',
      schedule: { id: 'paycom', label: 'Paycom' },
      unit: 'employee',
      count: (metrics) => metrics.employees,
    },
  ],
};
