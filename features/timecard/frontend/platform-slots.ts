import { CalendarDays } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Timecard puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  switch: { id: 'timecard', icon: CalendarDays },
  auditWording: wording,
  readToggles: {
    label: 'Timecard',
    missing: 'timecard data',
    order: 20,
    sources: { timecards: 'Timecard', meal_breaks: 'Meal Breaks' },
    toggles: [
      { id: 'timecards', label: 'Timecards', missing: 'timecards', source: 'timecards' },
      { id: 'meal_breaks', label: 'Meal breaks', missing: 'meal breaks', source: 'meal_breaks' },
    ],
  },
};
