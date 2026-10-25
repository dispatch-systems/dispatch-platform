import { useEffect, useRef } from 'react';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
import { useCachedData } from '../../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../../core/shell/frontend/runtime/data-cache.js';
import {
  hashQuery,
  navigate,
  pageHash,
  parseHash,
} from '../../../../core/shell/frontend/runtime/navigation.js';
import { settingsRead, type Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import { getWaiting, waitingUrl } from '../../api/client.js';
import type { CommunicationsWaiting } from '../../api/index.js';
import { ownDisplay } from '../display/display.js';
import { preview } from '../format.js';
import { arrived, keepSeen, readSeen, type InFront } from './arrivals.js';
import { chime, readyOnUse } from './chime.js';
import { lead } from './leading.js';
import { notify, notifying } from './notices.js';

// Alerts the member to each new message from an employee, wherever they are in Dispatch, as they
// chose in Message display: a chime, the browser's notification, or both. One of their tabs at
// the DSP alerts them (`lead`), in view or not, looking at what waits every few seconds: what
// this tab read in the last half minute, else read again.

/** How often the alerting tab looks at what waits. */
const LOOK_MS = 5_000;

/** The conversation or group open on Communications' page, while it's in front of the member. */
function inFront(): InFront {
  if (document.visibilityState !== 'visible' || !document.hasFocus()) return {};
  if (parseHash(location.hash).page !== 'communications') return {};
  const query = hashQuery();
  return { conversation: query.get('open') ?? undefined, group: query.get('group') ?? undefined };
}

export function MessageAlerts(access: Watching) {
  // Read as the tab opens and comes back into view, and as the member saves them here.
  const { data: settings } = useCachedData<SettingsAnswer>(settingsRead, -1);
  const { sound, notifications } = ownDisplay(settings);
  const chosen = { sound, notifications: notifications && notifying() };
  const latest = useRef({ chosen, access });
  latest.current = { chosen, access };
  const on = chosen.sound || chosen.notifications;
  const dsp = access.view.dsp.id;
  const user = access.session.user.id;
  useEffect(() => (sound ? readyOnUse() : undefined), [sound]);
  useEffect(() => {
    if (!on) return;
    /** Alerts of each message new in what waits, as the member chose. */
    const heard = (waiting: CommunicationsWaiting) => {
      const before = readSeen(dsp, user);
      const { fresh, seen } = arrived(waiting.latest, before, Date.now(), inFront());
      keepSeen(dsp, user, seen);
      const { chosen, access } = latest.current;
      if (fresh.length && chosen.sound) void chime();
      if (!chosen.notifications) return;
      for (const each of fresh) {
        const opens = pageHash(access, 'communications', { open: each.conversationId });
        notify(each.from, preview(each.message), () => {
          if (opens) navigate(opens);
        });
      }
    };
    const look = () => void dataCache.read(waitingUrl, getWaiting).then(heard, () => undefined);
    let looking: ReturnType<typeof setInterval> | undefined;
    return lead(`dispatch-communications-alerts:${dsp}:${user}`, (leads) => {
      clearInterval(looking);
      if (!leads) return;
      look();
      looking = setInterval(look, LOOK_MS);
    });
  }, [on, dsp, user]);
  return null;
}
