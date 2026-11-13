import { useEffect, useSyncExternalStore } from 'react';
import type {
  DspView,
  PreferenceSaved,
  SidebarArrangement,
} from '../../../accounts/api/index.js';
import { messageOf } from '../lib/errors.js';
import { api, view, viewing } from './api.js';
import { useFeedback } from './feedback.js';
import { PreferenceStore, type Change } from './preference-store.js';

/**
 * Every preference a member keeps, by id, with its value's type: core's here, and each owner's
 * added by declaration merging, as the backend declares it (`manifest::preferences`).
 */
export interface Preferences {
  'shell.sidebar': SidebarArrangement;
}
export type PreferenceId = Extract<keyof Preferences, string>;
export type { Change };

// A save goes only through the view it was read from, so none reaches a DSP left meanwhile.
const store = new PreferenceStore((token, id, body) =>
  viewing(view) === viewing(token)
    ? api<PreferenceSaved>(`/api/dsp/preferences/${id}`, body)
    : Promise.resolve(undefined),
);

/**
 * The member's own `id` in the DSP of `view`, none until they change it, and `change`, which
 * shows a change at once and keeps it behind; one that can't be kept says why.
 */
export function usePreference<Id extends PreferenceId>(view: DspView | undefined, id: Id) {
  const { fail } = useFeedback();
  const preference = view && store.of(view, id);
  useEffect(() => {
    if (preference) preference.failed = (error) => fail(messageOf(error));
  }, [preference, fail]);
  const value = useSyncExternalStore(store.subscribe, () => preference?.shown) as
    | Preferences[Id]
    | undefined;
  return {
    value,
    change: (change: Change<Preferences[Id]>) =>
      preference && store.change(preference, change as Change<unknown>),
  };
}
