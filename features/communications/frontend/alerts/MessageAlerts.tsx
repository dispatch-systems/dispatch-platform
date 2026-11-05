import { useEffect, useRef } from 'react';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
import { useCachedData } from '../../../../core/shell/frontend/runtime/api.js';
import { settingsRead, type Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import { ownDisplay } from '../display/display.js';
import { alertMember, choices } from './alerting.js';
import { readyOnUse } from './chime.js';
import { allowed } from './notices.js';
import { followPush } from './push.js';

// Alerts the member to each new message from an employee, wherever they are in Dispatch, as they
// chose in Message display: a chime, the browser's notification, or both (`alertMember`); and has
// this browser pushed to as they chose, for when Dispatch is closed (`followPush`).

export function MessageAlerts(access: Watching) {
  // Read as the tab opens and comes back into view, and as the member saves them here; till then,
  // as in a tab opened out of view, the defaults, as each turn reads them itself before alerting.
  const { data: settings } = useCachedData<SettingsAnswer>(settingsRead, -1);
  const { sound, notifications } = choices(settings);
  const latest = useRef(access);
  latest.current = access;
  const on = sound || notifications;
  const dsp = access.view.dsp.id;
  const user = access.session.user.id;
  // Pushed to once what they chose is read, not by the defaults before it.
  const pushed = settings && ownDisplay(settings).notifications && allowed();
  useEffect(() => (sound ? readyOnUse() : undefined), [sound]);
  useEffect(() => (on ? alertMember(() => latest.current) : undefined), [on, dsp, user]);
  useEffect(() => {
    // Not followed now, as offline, it's followed as the next tab opens.
    if (pushed !== undefined) followPush(pushed).catch(() => {});
  }, [pushed, dsp, user]);
  return null;
}
