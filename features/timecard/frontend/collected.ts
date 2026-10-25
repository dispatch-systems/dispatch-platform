import {
  Building,
  Calendar,
  CalendarDays,
  CircleCheck,
  CircleDot,
  Clock,
  Flag,
  Hash,
  IdCard,
  ListChecks,
  MapPin,
  Package,
  Route,
  ScanEye,
  Timer,
  UserRound,
  Users,
  Utensils,
} from 'lucide-react';
import type { Collected } from '../../../core/shell/frontend/runtime/slots.js';

// What the collections Timecard keeps collect, as Settings' Collectors page shows them: Paycom's
// timecards and Cortex's meal breaks. Each value is made up, to show what a field holds.

export const collected: Record<string, Collected> = {
  paycom: {
    from: ['Time & Attendance', 'Timecard Search'],
    records: [
      {
        label: 'Employees',
        icon: Users,
        counted: 'employees',
        fields: [
          { label: 'Employee code', icon: Hash, example: '1042' },
          { label: 'Name', icon: UserRound, example: 'Marcus Reyes' },
          { label: 'Department', icon: Building, example: 'Delivery' },
          { label: 'Position', icon: IdCard, example: 'Delivery Associate' },
          { label: 'Station', icon: MapPin, example: 'DLA7' },
          { label: 'Active', icon: CircleDot, example: 'Yes' },
        ],
      },
      {
        label: 'Timecards',
        icon: CalendarDays,
        counted: 'timecards',
        fields: [
          { label: 'Date', icon: Calendar, example: 'Sat, Oct 24' },
          { label: 'Punches', icon: Clock, example: '6:58 AM – 11:30 AM, 12:00 PM – 5:12 PM' },
          { label: 'Hours', icon: Timer, example: '9.7' },
          { label: 'Status', icon: ListChecks, example: 'Complete' },
        ],
      },
    ],
    period: [{ label: 'Current pay period', detail: 'Every employee' }],
    each: 'Replaces the pay period',
    dropped: ['Pay amounts'],
  },
  meal_break: {
    from: ['Operations', 'Execution', 'Itineraries'],
    records: [
      {
        label: 'Itineraries',
        icon: Route,
        counted: 'itineraries',
        fields: [
          { label: 'Transporter ID', icon: Hash, example: 'A2KX9P3LQ7ZT' },
          { label: 'Driver', icon: UserRound, example: 'Marcus Reyes' },
          { label: 'Route', icon: Route, example: 'CX214' },
          { label: 'Route complete', icon: CircleCheck, example: 'No' },
          { label: 'Delivery coverage', icon: ScanEye, example: 'Complete' },
          { label: 'Meal rule broken', icon: Flag, example: 'None' },
        ],
      },
      {
        label: 'Meals',
        icon: Utensils,
        counted: 'meals',
        fields: [
          { label: 'Start', icon: Clock, example: '11:42 AM' },
          { label: 'End', icon: Clock, example: '12:12 PM' },
          { label: 'Last delivery before', icon: Package, example: '11:38 AM · Stop 87' },
          { label: 'First delivery after', icon: Package, example: '12:19 PM · Stop 88' },
        ],
      },
    ],
    period: [{ label: 'Today', detail: 'Every station' }],
    each: 'Replaces today’s read',
    skips: ['Finished routes already read'],
  },
};
