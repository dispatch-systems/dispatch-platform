import {
  BadgeCheck,
  Contact,
  Hash,
  IdCard,
  Image,
  ListChecks,
  Mail,
  Phone,
  Smartphone,
  UserRound,
  Users,
} from 'lucide-react';
import type { Collected } from '../../../core/shell/frontend/runtime/slots.js';

// What the roster collects, as Settings' Collectors page shows it: everyone Amazon lists for the
// DSP, offboarded included. Each value is made up, to show what a field holds.

export const collected: Record<string, Collected> = {
  associates: {
    from: ['DA Console', 'Associates · Offboarded'],
    records: [
      {
        label: 'Associates',
        icon: Contact,
        counted: 'rows',
        fields: [
          { label: 'Transporter ID', icon: Hash, example: 'A2KX9P3LQ7ZT' },
          { label: 'Name', icon: UserRound, example: 'Marcus Reyes' },
          { label: 'Status', icon: ListChecks, example: 'Active' },
          { label: 'Roles', icon: Users, example: 'Driver' },
          { label: 'Qualifications', icon: BadgeCheck, example: 'CDV, EDV' },
          { label: 'Work phone', icon: Smartphone, example: '(323) 555-0182' },
          { label: 'Email', icon: Mail, example: 'marcus.reyes@example.com', personal: true },
          { label: 'Personal phone', icon: Phone, example: '(562) 555-0119', personal: true },
          { label: 'License expiry', icon: IdCard, example: 'Mar 14, 2028', personal: true },
          { label: 'Photo', icon: Image, example: 'Up to 400 px', personal: true },
        ],
      },
    ],
    period: [{ label: 'Whole roster', detail: 'Offboarded included' }],
    each: 'Replaces the roster, keeping people no longer listed',
    skips: ['Photos it already has'],
  },
};
