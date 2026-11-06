import { useEffect, useRef, useState } from 'react';
import { saveSettings, useSettings } from '../../../../core/settings/api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../../core/shell/frontend/runtime/data-cache.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { MessageDisplay } from '../../api/index.js';
import { ownDisplay, section } from './display.js';
import { autosave } from './saving.js';

/** How long after a choice it's saved, so a slider dragged saves once. */
const SAVE_MS = 400;

/**
 * The member's choices as they make them, saved by `autosave` through the page's action, which says
 * why a save failed at the top of the page until one succeeds; one refused because another session
 * saved first reads Settings again, and what it holds replaces the choices made.
 */
export function useDisplayChoices(refresh: () => void) {
  const { data: settings } = useSettings();
  const saved = ownDisplay(settings);
  const held = settings?.sections.find((each) => each.id === section);
  const [draft, setDraft] = useState<MessageDisplay>();
  const action = useAction((work: () => Promise<void>) => work());
  // The newest of each, for saves that run after the page draws again.
  const run = useRef(action.run);
  run.current = action.run;
  const reread = useRef(refresh);
  reread.current = refresh;
  const [saver] = useState(() =>
    autosave({
      save: async (values, revision) => {
        const answer = await saveSettings(section, { values, revision });
        return 'section' in answer ? answer.section.revision : revision;
      },
      view: () => dataCache.session,
      perform: (work) => run.current(work),
      later: (then) => {
        const timer = window.setTimeout(then, SAVE_MS);
        return () => window.clearTimeout(timer);
      },
      shown: setDraft,
      clash: (error) =>
        error instanceof ApiError && error.code === 'settings_changed_reload_before_saving',
      reread: () => reread.current(),
    }),
  );
  // Leaving the page saves what waits.
  useEffect(() => () => void saver.flush(), [saver]);
  const change = (changes: Partial<MessageDisplay>) =>
    saver.change(changes, { values: saved, revision: held?.revision ?? 0 });
  return { values: draft ?? saved, change };
}
