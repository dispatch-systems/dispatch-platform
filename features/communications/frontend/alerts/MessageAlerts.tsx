import { useEffect, useRef } from 'react';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
import { useCachedData } from '../../../../core/shell/frontend/runtime/data-reads.js';
import { settingsRead, type Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import { ownDisplay } from '../display/display.js';
import { alertMember, choices, inFront } from './alerting.js';
import { readyOnUse } from './chime.js';
import { allowed } from './notices.js';
import { alertsInFront, answerWorker, followPush } from './push.js';
import { DISPATCH_PHONE, type TextingLine } from '../line.js';

// Alerts the member to each new message from an employee on a line, wherever they are in Dispatch,
// as they chose in its section of Display: a chime, the browser's notification, or both (`alertMember`); and has
// this browser pushed to as they chose, for when Dispatch is closed (`followPush`), telling the
// service worker, while it's in front, which pushes it alerts of itself (`answerWorker`).

export function MessageAlerts({
  line = DISPATCH_PHONE,
  ...access
}: Watching & { line?: TextingLine }) {
  // Read as the tab opens and comes back into view, and as the member saves them here; till then,
  // as in a tab opened out of view, the defaults, as each turn reads them itself before alerting.
  const { data: settings } = useCachedData<SettingsAnswer>(settingsRead, -1);
  const { sound, notifications } = choices(settings, line.display);
  const latest = useRef(access);
  latest.current = access;
  const notifies = useRef(notifications);
  notifies.current = notifications;
  const on = sound || notifications;
  const dsp = access.view.dsp.id;
  const user = access.session.user.id;
  // Pushed to once what they chose is read, not by the defaults before it.
  const pushed = settings && ownDisplay(settings, line.display).notifications && allowed();
  useEffect(() => (sound ? readyOnUse() : undefined), [sound]);
  useEffect(
    () => (on ? alertMember(() => latest.current, line) : undefined),
    [on, dsp, user, line],
  );
  useEffect(() => {
    if (pushed === undefined) return;
    // Ended as the page moves on to another DSP or member, or they choose again; not followed
    // now, as offline, it's followed as the next tab opens.
    const ends = new AbortController();
    followPush(pushed, dsp, ends.signal, line).catch(() => {});
    return () => ends.abort();
  }, [pushed, dsp, user, line]);
  useEffect(
    () =>
      answerWorker((push) =>
        alertsInFront(push, dsp, inFront(line.page), notifies.current, line.page),
      ),
    [dsp, line],
  );
  return null;
}
