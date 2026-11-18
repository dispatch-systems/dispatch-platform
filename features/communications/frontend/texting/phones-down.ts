import { settingsHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import type { Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import type { DispatchPhone } from '../../api/index.js';
import type { TextingLine } from '../line.js';

// A line's phones that are down, as every page's banner and a notification say: one offline for
// ten minutes or more, or one that says it can't send texts.

/** How long a phone is offline before it's down. */
export const OFFLINE_MS = 10 * 60_000;

/** A phone that's down: how, what says it, and by its `key`, each time it went down. */
export type Down = { kind: 'offline' | 'cant_send'; key: string; words: string };

/** What says which line's phones: Dispatch Phone's own, by default, or another's. */
type Called = Pick<TextingLine, 'id' | 'name' | 'phone'>;
const DISPATCH: Called = { name: 'Dispatch Phone', phone: 'Dispatch phone' };

/** The `phones` of `line` an app holds that are down at `now`: by name while there are several,
 * else as the line calls its phone; another line's by its name too, unless the phone's says it,
 * as its alerts always say the line (HR Line · Pixel). */
export function down(
  phones: readonly DispatchPhone[],
  now: number,
  line: Called = DISPATCH,
): Down[] {
  const named = (phone: DispatchPhone) => {
    if (phones.length === 1) return line.phone;
    if (line.id === undefined || phone.name.startsWith(line.name)) return phone.name;
    return `${line.name} · ${phone.name}`;
  };
  return phones.flatMap((phone): Down[] => {
    const since = phone.lastSeenAt;
    if (!phone.online && since !== null && now - since >= OFFLINE_MS)
      return [
        {
          kind: 'offline',
          key: `${phone.id}:offline:${since}`,
          words: `${named(phone)} offline for ${lasted(now - since)}`,
        },
      ];
    if (phone.cantSend)
      return [
        {
          kind: 'cant_send',
          key: `${phone.id}:cant_send`,
          words: `${named(phone)} can't send texts`,
        },
      ];
    return [];
  });
}

/** How long it's been, in plain words: "25 min", "3 hours", "2 days". */
function lasted(ms: number) {
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return hours === 1 ? '1 hour' : `${hours} hours`;
  const days = Math.floor(hours / 24);
  return days === 1 ? '1 day' : `${days} days`;
}

/** The line's phones' card on DSP Connections, for whoever manages them or the DSP's connections. */
export function phonesCard({ view }: Watching, line: TextingLine) {
  if (!can(view, 'connections.manage') && !can(view, line.phones)) return undefined;
  return settingsHash(view.dsp.id, 'connections', view);
}
