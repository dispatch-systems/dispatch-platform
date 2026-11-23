import {
  CalendarDays,
  Clock,
  Hash,
  ListChecks,
  MessageSquare,
  Tag,
  Timer,
  UserRound,
} from 'lucide-react';
import type { Collected } from '../../../core/shell/frontend/runtime/slots.js';

// What Schedules collects, as Settings' Collectors page shows it: every shift assignment the
// Scheduling week view lists at the DSP's station. Each value is made up, to show what a field
// holds.

export const collected: Record<string, Collected> = {
  shifts: {
    from: ['Scheduling', 'Calendar', 'Week'],
    records: [
      {
        label: 'Shifts',
        icon: CalendarDays,
        counted: 'rows',
        fields: [
          { label: 'Driver', icon: UserRound, example: 'Marcus Reyes' },
          { label: 'Transporter ID', icon: Hash, example: 'A2K9X7LMQ4R8T' },
          { label: 'Day', icon: CalendarDays, example: 'Tue, Nov 24' },
          { label: 'Shift', icon: Tag, example: 'Standard Parcel' },
          { label: 'Assignment', icon: ListChecks, example: 'Primary' },
          { label: 'Start', icon: Clock, example: '9:45 AM' },
          { label: 'Length', icon: Timer, example: '8 h 20 min' },
          { label: 'Note', icon: MessageSquare, example: 'Training ride' },
        ],
      },
    ],
    period: [{ label: 'This week', detail: 'Sunday to Saturday' }],
    each: 'Keeps every change to the week',
    dropped: ['Rostered work blocks', 'Email', 'Phone'],
  },
};
