import { useEffect, useRef, useState } from 'react';
import { hashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';

// What Slack sent the browser back with, after an install started on Settings' Connections,
// and finishing it there.

type SlackReturn = { state: string; code: string; error: string };
/** What Slack sent back, until the card starts finishing it. */
let pending: SlackReturn | undefined;
const FIELDS = ['slackState', 'slackCode', 'slackError'];

/**
 * What Slack sent the browser back with, taken out of the address so the code is neither used
 * twice nor left in the browser's history; the rest of the address stays. It is kept until the
 * card starts finishing it: React may set aside a first render and render it again, and that
 * render finds the address already clean.
 */
function takeSlackReturn() {
  const query = hashQuery();
  const state = query.get('slackState');
  if (state) {
    pending = { state, code: query.get('slackCode') ?? '', error: query.get('slackError') ?? '' };
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
/** Why Slack sent the browser back without a code. */
const cancelled = (error: string) =>
  error === 'access_denied' ? "Slack wasn't connected." : "Slack didn't finish. Try again.";

/**
 * Finishes the install Slack sent the browser back from with `finish`: once, as soon as the
 * card is drawn. Whether it's finishing, and why it didn't, if it didn't.
 */
export function useSlackReturn(finish: (state: string, code: string) => Promise<void>) {
  const [returned] = useState(takeSlackReturn);
  const finishing = useAction(finish, { success: 'Slack connected', inline: true });
  const started = useRef(false);
  useEffect(() => {
    if (!returned) return;
    // A later visit doesn't find it.
    if (returned === pending) pending = undefined;
    if (!returned.code || started.current) return;
    started.current = true;
    void finishing.run(returned.state, returned.code);
  }, [returned, finishing]);
  return {
    busy: finishing.busy,
    notice: (returned?.error && cancelled(returned.error)) || finishing.error,
  };
}
