import { useSyncExternalStore } from 'react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import type { SettingsValue } from '../../../core/settings/api/index.js';
import { viewing } from '../../../core/shell/frontend/runtime/api.js';

// The sections' unsaved edits are kept beside Settings rather than in it, so they outlive its
// view opening again for the same member, DSP and role viewed as, as it does once another admin
// edits roles or the DSP's profile is saved: drawn anew, the page has them as they were, each at
// the revision it began at, so a save another session made meanwhile still refuses them. They're
// kept until saved, or until the page moves on; another member, DSP or role viewed as has none of
// them.

/** A section's edits: each field edited, by its id, and the revision of the section they began at. */
export type Draft = { revision: number; values: Record<string, SettingsValue> };

/** Whose Settings is open: the member's, in the view's DSP, as the role viewed as. */
export const whoseSettings = (session: SessionView, view: DspView) =>
  `${session.user.id} ${viewing(view.token)}`;

let kept = { whose: '', drafts: new Map<string, Draft>() };
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => void listeners.delete(listener);
};
const changed = () => listeners.forEach((listener) => listener());

/** A section's edits in `whose` Settings, and what changes them, as `useState` does. */
export function useDraft(whose: string, section: string) {
  const draft = useSyncExternalStore(subscribe, () =>
    kept.whose === whose ? kept.drafts.get(section) : undefined,
  );
  const change = (next: Draft | undefined | ((draft: Draft | undefined) => Draft | undefined)) => {
    const ours = kept.whose === whose;
    const edits =
      typeof next === 'function' ? next(ours ? kept.drafts.get(section) : undefined) : next;
    // A save that ends once another's Settings is open leaves that one's edits alone.
    if (!ours && !edits) return;
    if (!ours) kept = { whose, drafts: new Map() };
    if (edits) kept.drafts.set(section, edits);
    else kept.drafts.delete(section);
    changed();
  };
  return [draft, change] as const;
}
/** Forgets every section's edits, as the page moves on once its editors let it. */
export function forgetDrafts() {
  kept.drafts.clear();
  changed();
}
