import { useEffect, useRef, useState } from 'react';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { hashQuery } from '../../../../core/shell/frontend/runtime/navigation.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';

// What Slack sent the browser back with, after an install started on Settings' Connections, or
// on the page that named itself to `connectSlack`, and finishing it there. Why one didn't finish
// shows at the top of the page: a workspace being added has no card yet to show it on.

type SlackReturn = { state: string; code: string; error: string };
/** What Slack sent back, until the card starts finishing it. */
let pending: SlackReturn | undefined;
const FIELDS = ['slackState', 'slackCode', 'slackError'];

/**
 * What Slack sent the browser back with, taken out of the address so the code is neither used
 * twice nor left in the browser's history; the rest of the address stays. It is kept until the
 * page starts finishing it: React may set aside a first render and render it again, and that
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
 * Finishes the install Slack sent the browser back from with `finish`: once, as soon as what
 * calls it is drawn, as Slack's cards on Connections are. Whether it's finishing.
 */
export function useSlackReturn(finish: (state: string, code: string) => Promise<void>) {
  const [returned] = useState(takeSlackReturn);
  const { fail } = useFeedback();
  const finishing = useAction(finish, { success: 'Slack connected' });
  const started = useRef(false);
  useEffect(() => {
    if (!returned) return;
    // A later visit doesn't find it.
    if (returned === pending) pending = undefined;
    if (started.current) return;
    started.current = true;
    if (returned.code) void finishing.run(returned.state, returned.code);
    else fail(cancelled(returned.error));
  }, [returned, finishing, fail]);
  return { busy: finishing.busy };
}
