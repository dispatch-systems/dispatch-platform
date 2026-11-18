import { getSettings, readSettings } from '../../../../core/settings/api/client.js';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
import { dataCache } from '../../../../core/shell/frontend/runtime/data-cache.js';
import {
  hashQuery,
  pageHash,
  parseHash,
} from '../../../../core/shell/frontend/runtime/navigation.js';
import { settingsRead, type Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import type { CommunicationsWaiting } from '../../api/index.js';
import { ownDisplay } from '../display/display.js';
import { DISPATCH_PHONE, keptAt, own, type TextingLine } from '../line.js';
import { preview } from '../shared/format.js';
import { down, phonesCard } from '../texting/phones-down.js';
import {
  arrived,
  keepDowns,
  keepSeen,
  readDowns,
  readSeen,
  wentDown,
  type InFront,
} from './arrivals.js';
import { chime } from './chime.js';
import { lead } from './leading.js';
import { notify, notifying } from './notices.js';

// One of the member's tabs at the DSP alerts them (`lead`), for each line apart, in view or not, of
// new messages and of the line's phones as each goes down, looking at what waits every few seconds: what this tab read in the last half minute, else read again. Each turn
// first reads what the member chose again, as they may have changed it in another tab since this
// one read it, and alerts as they last chose; what's read for a turn that ended alerts no one.

/** How often the alerting tab looks at what waits. */
const LOOK_MS = 5_000;

/** How the member chose to be alerted in this browser of the line's messages, by its Display
 * section, `section`, in their Settings as read. */
export function choices(settings: SettingsAnswer | undefined, section: string) {
  const { sound, notifications } = ownDisplay(settings, section);
  return { sound, notifications: notifications && notifying() };
}

/** The conversation or group open on the line's page, `page`, while it's in front of the member. */
export function inFront(page: string): InFront {
  if (document.visibilityState !== 'visible' || !document.hasFocus()) return {};
  if (parseHash(location.hash).page !== page) return {};
  const query = hashQuery();
  return { conversation: query.get('open') ?? undefined, group: query.get('group') ?? undefined };
}

/**
 * Has this tab alert the member of each new message from an employee on `line`, in its turns,
 * until the answer is called. `access` is theirs as last drawn, of one DSP and member throughout.
 * Another feature's line's say its name before who wrote, as its pushes do.
 */
export function alertMember(access: () => Watching, line: TextingLine = DISPATCH_PHONE) {
  const dsp = access().view.dsp.id;
  const user = access().session.user.id;
  // What the member's tabs saw of the line, and the turn alerting them of it, are the line's own.
  const at = keptAt(line, dsp);
  const { texting } = line;
  const titled = (from: string) => (own(line) ? from : `${line.name} · ${from}`);
  /** Alerts of each message new in what waits, and each phone gone down, as the member chose. */
  const heard = (waiting: CommunicationsWaiting, settings: SettingsAnswer) => {
    const now = Date.now();
    const { fresh, seen } = arrived(waiting.latest, readSeen(at, user), now, inFront(line.page));
    keepSeen(at, user, seen);
    const phones = wentDown(down(waiting.phones, now, line), readDowns(at, user), now);
    keepDowns(at, user, phones.seen);
    const chosen = choices(settings, line.display);
    if (fresh.length && chosen.sound) void chime();
    if (!chosen.notifications) return;
    const card = phonesCard(access(), line);
    for (const each of phones.fresh) notify(each.words, '', `phone-down:${each.key}`, card);
    for (const each of fresh) {
      const opens = pageHash(access(), line.page, { open: each.conversationId });
      notify(titled(each.from), preview(each.message), each.message.id, opens);
    }
  };
  return lead(`dispatch-communications-alerts:${at}:${user}`, (ends) => {
    /** What the member chose, as this turn reads it again: not as a read begun before it has. */
    let chosen: Promise<SettingsAnswer> | undefined;
    const look = async () => {
      if (!chosen) {
        dataCache.invalidate((key) => key === settingsRead);
        const reading = dataCache.read(settingsRead, getSettings);
        // Failed, it's read again at the next look.
        reading.catch(() => {
          if (chosen === reading) chosen = undefined;
        });
        chosen = reading;
      }
      try {
        const [, waiting] = await Promise.all([
          chosen,
          dataCache.read(texting.waitingUrl, texting.getWaiting),
        ]);
        // Then as they chose last, as the tab holds it, else as read again once let go.
        const settings = readSettings() ?? (await dataCache.read(settingsRead, getSettings));
        if (!ends.aborted) heard(waiting, settings);
      } catch {
        // Not read, what waits is looked at again at the next look.
      }
    };
    void look();
    const looking = setInterval(() => void look(), LOOK_MS);
    ends.addEventListener('abort', () => clearInterval(looking));
  });
}
