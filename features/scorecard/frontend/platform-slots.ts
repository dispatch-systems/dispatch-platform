import { Award } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Scorecard puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  switch: { id: 'scorecard', icon: Award },
  auditWording: wording,
  readToggles: {
    label: 'Scorecard',
    missing: 'scorecard data',
    order: 50,
    sources: { scorecard: 'Scorecard' },
    toggles: [
      {
        id: 'feedback',
        label: 'Customer feedback',
        missing: 'customer feedback',
        source: 'scorecard',
      },
      { id: 'safety', label: 'Safety events', missing: 'safety events', source: 'scorecard' },
      {
        id: 'returns',
        label: 'Returns & contact compliance',
        missing: 'returns',
        source: 'scorecard',
      },
      {
        id: 'scorecard',
        label: 'Weekly scorecard',
        missing: 'weekly scorecard',
        source: 'scorecard',
      },
    ],
  },
};
