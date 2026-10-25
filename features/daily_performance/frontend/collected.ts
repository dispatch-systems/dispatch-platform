import {
  BadgeCheck,
  Boxes,
  Building2,
  MessageSquare,
  Navigation,
  PhoneCall,
  Ruler,
  Shield,
  ShieldAlert,
  Smartphone,
  Undo2,
  Users,
} from 'lucide-react';
import type { Collected } from '../../../core/shell/frontend/runtime/slots.js';

// What Daily Performance collects, as Settings' Collectors page shows it: Daily Quality's and Day
// Safety's datasets, each kept as Cortex sends its rows. Each value is made up, to show what one
// holds.

export const collected: Record<string, Collected> = {
  daily_performance: {
    from: ['Performance', 'Daily Quality · Day Safety'],
    datasets: true,
    records: [
      {
        label: 'Drivers',
        icon: Users,
        fields: [
          { label: 'Quality', icon: BadgeCheck, example: '448 rows' },
          { label: 'Returns', icon: Undo2, example: '448 rows' },
          { label: 'Returns to station', icon: Undo2, example: '96 rows' },
          { label: 'Delivery behaviors', icon: Navigation, example: '61 rows' },
          { label: 'Customer feedback', icon: MessageSquare, example: '73 rows' },
          { label: 'Pickups', icon: Boxes, example: '448 rows' },
          { label: 'Customer contacts', icon: PhoneCall, example: '448 rows' },
          { label: 'Thresholds', icon: Ruler, example: '7 rows' },
          { label: 'Safety, so far', icon: Shield, example: '64 rows' },
          { label: 'Safety, end of day', icon: Shield, example: '384 rows' },
          { label: 'Safety events', icon: ShieldAlert, example: '212 rows' },
          { label: 'Live safety events', icon: ShieldAlert, example: '9 rows' },
        ],
      },
      {
        label: 'DSP',
        icon: Building2,
        fields: [
          { label: 'Quality', icon: BadgeCheck, example: '7 rows' },
          { label: 'Returns', icon: Undo2, example: '7 rows' },
          { label: 'Delivery behaviors', icon: Navigation, example: '7 rows' },
          { label: 'Customer feedback', icon: MessageSquare, example: '7 rows' },
          { label: 'Pickups', icon: Boxes, example: '7 rows' },
          { label: 'Customer contacts', icon: PhoneCall, example: '7 rows' },
          { label: 'Thresholds', icon: Ruler, example: '7 rows' },
          { label: 'Safety, so far', icon: Shield, example: '1 row' },
          { label: 'Safety, end of day', icon: Shield, example: '6 rows' },
          { label: 'Working devices', icon: Smartphone, example: '7 rows' },
        ],
      },
    ],
    period: [{ label: 'Days before today', detail: 'As many as Look back' }],
    each: 'Replaces each day it reads',
    skips: ['Days read within Refresh after'],
  },
};
