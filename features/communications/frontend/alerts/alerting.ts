import { getSettings, readSettings } from '../../../../core/settings/api/client.js';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
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
import { chime } from './chime.js';
import { lead } from './leading.js';
import { notify, notifying } from './notices.js';

// One of the member's tabs at the DSP alerts them (`lead`), in view or not, looking at what waits
// every few seconds: what this tab read in the last half minute, else read again. Each turn
// first reads what the member chose again, as they may have changed it in another tab since this
// one read it, and alerts as they last chose; what's read for a turn that ended alerts no one.

/** How often the alerting tab looks at what waits. */
const LOOK_MS = 5_000;

/** How the member chose to be alerted in this browser, by their Settings as read. */
export function choices(settings: SettingsAnswer | undefined) {
  const { sound, notifications } = ownDisplay(settings);
  return { sound, notifications: notifications && notifying() };
}

/** The conversation or group open on Communications' page, while it's in front of the member. */
function inFront(): InFront {
  if (document.visibilityState !== 'visible' || !document.hasFocus()) return {};
  if (parseHash(location.hash).page !== 'communications') return {};
  const query = hashQuery();
  return { conversation: query.get('open') ?? undefined, group: query.get('group') ?? undefined };
}

/**
 * Has this tab alert the member of each new message from an employee, in its turns, until the
 * answer is called. `access` is theirs as last drawn, of one DSP and member throughout.
 */
export function alertMember(access: () => Watching) {
  const dsp = access().view.dsp.id;
  const user = access().session.user.id;
  /** Alerts of each message new in what waits, as the member chose. */
  const heard = (waiting: CommunicationsWaiting, settings: SettingsAnswer) => {
    const { fresh, seen } = arrived(waiting.latest, readSeen(dsp, user), Date.now(), inFront());
    keepSeen(dsp, user, seen);
    const chosen = choices(settings);
    if (fresh.length && chosen.sound) void chime();
    if (!chosen.notifications) return;
    for (const each of fresh) {
      const opens = pageHash(access(), 'communications', { open: each.conversationId });
      notify(each.from, preview(each.message), () => {
        if (opens) navigate(opens);
      });
    }
  };
  return lead(`dispatch-communications-alerts:${dsp}:${user}`, (ends) => {
    /** What the member chose, as this turn reads it again: not as a read begun before it has. */
    let chosen: Promise<SettingsAnswer> | undefined;
    const look = () => {
      if (!chosen) {
        dataCache.invalidate((key) => key === settingsRead);
        const reading = dataCache.read(settingsRead, getSettings);
        // Failed, it's read again at the next look.
        reading.catch(() => {
          if (chosen === reading) chosen = undefined;
        });
        chosen = reading;
      }
      void Promise.all([chosen, dataCache.read(waitingUrl, getWaiting)]).then(
        // As they chose last, should the tab have read it again since.
        ([read, waiting]) => {
          if (!ends.aborted) heard(waiting, readSettings() ?? read);
        },
        () => undefined,
      );
    };
    look();
    const looking = setInterval(look, LOOK_MS);
    ends.addEventListener('abort', () => clearInterval(looking));
  });
}
