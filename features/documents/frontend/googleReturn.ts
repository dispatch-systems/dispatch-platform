import { hashQuery } from '../../../core/shell/frontend/runtime/navigation.js';

// What Google sent the browser back with, after a sign-in started on Settings' Connections or
// on Documents. Its state says which sign-in it was: connecting the DSP's account, or linking
// a member's own.

export type GoogleReturn = { state: string; code: string; error: string };
/** What Google sent back, until the card or page it's for starts finishing it. */
let pending: GoogleReturn | undefined;
const FIELDS = ['googleState', 'googleCode', 'googleError'];

/**
 * What Google sent the browser back with, taken out of the address so the code is neither used
 * twice nor left in the browser's history; the rest of the address stays. It is kept until the
 * card or page it's for starts finishing it: React may set aside a first render and render it
 * again, and that render finds the address already clean.
 */
export function takeGoogleReturn() {
  const query = hashQuery();
  const state = query.get('googleState');
  if (state) {
    pending = { state, code: query.get('googleCode') ?? '', error: query.get('googleError') ?? '' };
    for (const field of FIELDS) query.delete(field);
    const [page] = location.hash.split('?');
    const rest = query.toString();
    history.replaceState(
      history.state,
      '',
      `${location.pathname}${location.search}${page}${rest ? `?${rest}` : ''}`,
    );
  }
  return pending;
}
/** Lets `returned` go, as the card or page it's for finishes it: a later visit doesn't find it. */
export function settle(returned: GoogleReturn) {
  if (returned === pending) pending = undefined;
}
/** Whether a sign-in linked a member's own Google account, rather than connecting the DSP's. */
export const isLink = (returned: GoogleReturn) => returned.state.includes('.link.');
/** Why Google sent the browser back without a code. */
export const cancelled = (error: string) =>
  error === 'access_denied'
    ? 'Google sign-in was cancelled, so nothing changed.'
    : "Google didn't finish signing in. Try again.";
