import {
  Award,
  BadgeCheck,
  Box,
  Boxes,
  Building2,
  ListChecks,
  MessageSquare,
  Shield,
  ShieldAlert,
  Smartphone,
  Undo2,
  Users,
} from 'lucide-react';
import type { Collected } from '../../../core/shell/frontend/runtime/slots.js';

// What the weekly scorecard collects, as Settings' Collectors page shows it: the scorecard's
// datasets, each kept as Cortex sends its rows. Each value is made up, to show what one holds.

export const collected: Record<string, Collected> = {
  weekly_scorecard: {
    from: ['Performance', 'Scorecard'],
    datasets: true,
    records: [
      {
        label: 'Drivers',
        icon: Users,
        fields: [
          { label: 'Weekly scorecard', icon: Award, example: '64 rows' },
          { label: 'Returns to station', icon: Undo2, example: '31 rows' },
          { label: 'Delivery concessions', icon: Box, example: '18 rows' },
          { label: 'Customer feedback', icon: MessageSquare, example: '22 rows' },
          { label: 'Pickup failures', icon: Boxes, example: '6 rows' },
          { label: 'Safety', icon: Shield, example: '64 rows' },
          { label: 'Safety events', icon: ShieldAlert, example: '47 rows' },
        ],
      },
      {
        label: 'DSP',
        icon: Building2,
        fields: [
          { label: 'Quality', icon: BadgeCheck, example: '1 row' },
          { label: 'Team', icon: Users, example: '1 row' },
          { label: 'Compliance', icon: ListChecks, example: '1 row' },
          { label: 'Safety', icon: Shield, example: '1 row' },
          { label: 'Working devices', icon: Smartphone, example: '1 row' },
          { label: 'Customer feedback', icon: MessageSquare, example: '1 row' },
          { label: 'Pickups', icon: Boxes, example: '1 row' },
        ],
      },
    ],
    period: [{ label: 'Completed weeks', detail: 'As many as Look back' }],
    each: 'Replaces each week it reads',
    skips: ['Weeks read within Refresh after'],
  },
};
