import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { Feature } from '../../../../tenancy/api/index.js';
import { present } from './installed.js';
import type { Access } from './pages.js';

// The account actions slot: what an owner adds to the account menu, beside Source code and
// Sign out.

/** An authenticated account action, with its own lazily loaded overlay and unread indicator. */
export type AccountAction = {
  id: string;
  label: string;
  part?: Feature;
  icon?: LucideIcon;
  visible?: (access: Access) => boolean;
  preload: () => Promise<unknown>;
  render: (context: Access & { onClose: () => void }) => ReactNode;
  indicator?: (context: Access) => ReactNode;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** Actions beside Source code and Sign out. */
    accountActions?: readonly AccountAction[];
  }
}

/** Account actions admitted for the current signed-in person and DSP view. */
export const accountActions = (access: Access) =>
  present(access.view, (feature) => feature.accountActions).filter(
    (action) => !action.visible || action.visible(access),
  );
