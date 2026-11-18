import { BriefcaseBusiness } from 'lucide-react';
import type {
  AuditPhrases,
  PlatformSlots,
} from '../../../../../core/shell/frontend/runtime/slots.js';

// What HR Line puts in the platform owner's slots, loaded with the platform owner's pages: its
// switch's icon, and how its events read in the audit log: its phones', each by the phone's name
// and number; and the names it saved and the numbers it blocked, each by who it's with and its
// number, never what a text says.

const phrases: AuditPhrases = {
  'cjf87gqd_hr_line.phones.paired': (e, { strong }) => ['paired HR Line phone ', strong(e.detail)],
  'cjf87gqd_hr_line.phones.renamed': (e, { strong }) => [
    'renamed HR Line phone ',
    strong(e.detail),
  ],
  'cjf87gqd_hr_line.phones.removed': (e, { strong }) => [
    'removed HR Line phone ',
    strong(e.detail),
  ],
  'cjf87gqd_hr_line.contact_saved': (e, { strong }) => [
    'saved an HR Line name for ',
    strong(e.target ?? 'a number'),
  ],
  'cjf87gqd_hr_line.number_blocked': (e, { strong }) => ['blocked ', strong(e.detail), ' on HR Line'],
  'cjf87gqd_hr_line.number_unblocked': (e, { strong }) => [
    'unblocked ',
    strong(e.detail),
    ' on HR Line',
  ],
};

export const slots: PlatformSlots = {
  switch: { id: 'cjf87gqd_hr_line', icon: BriefcaseBusiness },
  auditWording: { phrases, spoken: Object.keys(phrases) },
};
