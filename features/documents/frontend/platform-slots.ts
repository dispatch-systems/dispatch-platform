import { FolderOpen } from 'lucide-react';
import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Documents puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  switch: { id: 'documents', icon: FolderOpen },
  auditWording: wording,
  mailKinds: { 'documents.google_account': 'Documents: link a Google account' },
};
