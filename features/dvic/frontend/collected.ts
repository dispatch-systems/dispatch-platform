import {
  Calendar,
  ClipboardCheck,
  Clock,
  Hash,
  ListChecks,
  MapPin,
  Tag,
  Timer,
  Truck,
  UserRound,
} from 'lucide-react';
import type { Collected } from '../../../core/shell/frontend/runtime/slots.js';

// What DVIC collects, as Settings' Collectors page shows it: each vehicle inspection the weekly
// DVIC PreTrip reports list. Each value is made up, to show what a field holds.

export const collected: Record<string, Collected> = {
  dvic: {
    from: ['Performance', 'Supplementary Reports', 'DVIC PreTrip'],
    records: [
      {
        label: 'Inspections',
        icon: ClipboardCheck,
        counted: 'rows',
        fields: [
          { label: 'Date', icon: Calendar, example: 'Sat, Oct 24' },
          { label: 'Driver', icon: UserRound, example: 'Marcus Reyes' },
          { label: 'Transporter ID', icon: Hash, example: 'A2KX9P3LQ7ZT' },
          { label: 'VIN', icon: Truck, example: '1FTBW3XM6NKA04821' },
          { label: 'Fleet type', icon: Tag, example: 'CDV' },
          { label: 'Inspection type', icon: ClipboardCheck, example: 'Pre-trip' },
          { label: 'Status', icon: ListChecks, example: 'Completed' },
          { label: 'Start and end', icon: Clock, example: '10:12 AM – 10:16 AM' },
          { label: 'Duration', icon: Timer, example: '3 min 48 s' },
          { label: 'Station', icon: MapPin, example: 'DLA7' },
        ],
      },
    ],
    period: [{ label: 'This week and last', detail: 'And up to 2 older weeks not yet read' }],
    each: 'Adds to its history',
    skips: ['Reports unchanged since the last read'],
  },
};
