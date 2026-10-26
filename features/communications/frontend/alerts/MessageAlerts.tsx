import { useEffect, useRef } from 'react';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
import { useCachedData } from '../../../../core/shell/frontend/runtime/api.js';
import { settingsRead, type Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import { alertMember, choices } from './alerting.js';
import { readyOnUse } from './chime.js';

// Alerts the member to each new message from an employee, wherever they are in Dispatch, as they
// chose in Message display: a chime, the browser's notification, or both (`alertMember`).

export function MessageAlerts(access: Watching) {
  // Read as the tab opens and comes back into view, and as the member saves them here.
  const { data: settings } = useCachedData<SettingsAnswer>(settingsRead, -1);
  const { sound, notifications } = choices(settings);
  const latest = useRef(access);
  latest.current = access;
  // Nothing alerts before what the member chose is read.
  const on = settings !== undefined && (sound || notifications);
  const dsp = access.view.dsp.id;
  const user = access.session.user.id;
  useEffect(() => (sound ? readyOnUse() : undefined), [sound]);
  useEffect(() => (on ? alertMember(() => latest.current) : undefined), [on, dsp, user]);
  return null;
}
