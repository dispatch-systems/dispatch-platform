// The page's keys, on a desktop: J and K open the next and the previous conversation in the list,
// N Next waiting, E Done on the one open, / goes to Search, and Escape leaves the box written in.
// While a field is written in, only Escape is the page's; a key pressed with Ctrl, Alt or ⌘, or one
// something else has taken, never is.

export type Shortcut = 'next' | 'previous' | 'waiting' | 'done' | 'search' | 'leave';

/** The key of each that a button presses, as its `aria-keyshortcuts` and title name it. */
export const KEYS = { waiting: 'n', done: 'e' } as const;

const PRESSED: Readonly<Record<string, Shortcut>> = {
  j: 'next',
  k: 'previous',
  [KEYS.waiting]: 'waiting',
  [KEYS.done]: 'done',
  '/': 'search',
};

type Pressed = Pick<
  KeyboardEvent,
  'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'isComposing' | 'defaultPrevented'
>;

/** What `pressed` does on the page, while a field is written in (`writing`) or not. */
export function shortcut(pressed: Pressed, writing: boolean): Shortcut | undefined {
  if (pressed.defaultPrevented || pressed.isComposing) return undefined;
  if (pressed.ctrlKey || pressed.metaKey || pressed.altKey) return undefined;
  if (writing) return pressed.key === 'Escape' ? 'leave' : undefined;
  return PRESSED[pressed.key];
}

/** Whether `target` is something written in: a field, or what's edited in place. */
export const writingIn = (target: EventTarget | null) =>
  target instanceof HTMLElement &&
  (target.isContentEditable || target.closest('input, textarea, select') !== null);
