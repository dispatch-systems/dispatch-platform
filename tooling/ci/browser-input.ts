const MAX_SELECTOR_LENGTH = 512;

/** Turn the manual workflow's browser selector into one inert Playwright argument. */
export function manualBrowserSelection(value: string): string[] {
  const selector = value.trim();
  if (!selector) return [];
  if (selector.length > MAX_SELECTOR_LENGTH)
    throw new Error(`Browser selector must be at most ${MAX_SELECTOR_LENGTH} characters`);
  if (/[\u0000-\u001f\u007f]/u.test(selector))
    throw new Error('Browser selector must not contain control characters');
  if (selector.startsWith('-')) throw new Error('Browser selector must not start with an option');
  return [selector];
}
