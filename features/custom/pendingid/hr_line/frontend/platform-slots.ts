import { Phone } from 'lucide-react';
import type {
  AuditPhrases,
  PlatformSlots,
} from '../../../../../core/shell/frontend/runtime/slots.js';

// What HR Line puts in the platform owner's slots, loaded with the platform owner's pages: its
// switch's icon, and how its phones' events read in the audit log, each by the phone's name and
// number.

const phrases: AuditPhrases = {
  'pendingid_hr_line.phones.paired': (e, { strong }) => ['paired HR Line phone ', strong(e.detail)],
  'pendingid_hr_line.phones.renamed': (e, { strong }) => [
    'renamed HR Line phone ',
    strong(e.detail),
  ],
  'pendingid_hr_line.phones.removed': (e, { strong }) => [
    'removed HR Line phone ',
    strong(e.detail),
  ],
};

export const slots: PlatformSlots = {
  switch: { id: 'pendingid_hr_line', icon: Phone },
  auditWording: { phrases, spoken: Object.keys(phrases) },
};
