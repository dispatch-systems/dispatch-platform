import { useEffect, type RefObject } from 'react';
import { KEYS, shortcut, writingIn, type Shortcut } from '../shared/shortcuts.js';

// The page's keys at work (shared/shortcuts.ts), while `on`, as on a desktop: one a button names
// presses it, as the head's Done and Next waiting do; J and K open the row after or before the one
// open in the list, or its first with none open; / goes to Search; Escape leaves the field. Keys
// pressed in a dialog, or outside the page, are theirs.

export function useShortcuts(page: RefObject<HTMLElement | null>, on: boolean) {
  useEffect(() => {
    if (!on) return;
    const pressed = (event: KeyboardEvent) => {
      const element = page.current;
      const target = event.target instanceof Element ? event.target : null;
      const here = target === document.body || (target !== null && element?.contains(target));
      if (!element || !here || target?.closest('[role="dialog"]')) return;
      const action = shortcut(event, writingIn(target));
      if (!action) return;
      if (action === 'leave') return (target as HTMLElement).blur();
      event.preventDefault();
      perform(element, action);
    };
    document.addEventListener('keydown', pressed);
    return () => document.removeEventListener('keydown', pressed);
  }, [page, on]);
}

function perform(page: HTMLElement, action: Exclude<Shortcut, 'leave'>) {
  if (action === 'waiting' || action === 'done')
    return page
      .querySelector<HTMLButtonElement>(`.thread-head [aria-keyshortcuts="${KEYS[action]}"]`)
      ?.click();
  if (action === 'search')
    return page.querySelector<HTMLInputElement>('.pane-list .search input')?.focus();
  const rows = [...page.querySelectorAll<HTMLButtonElement>('.pane-list .convo-open')];
  const at = rows.findIndex((row) => row.getAttribute('aria-current') === 'true');
  const to = at < 0 ? rows[0] : rows[action === 'next' ? at + 1 : at - 1];
  to?.focus();
  to?.click();
}
