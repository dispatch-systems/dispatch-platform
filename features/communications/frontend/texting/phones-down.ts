import { settingsHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import type { Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import type { DispatchPhone } from '../../api/index.js';

// The DSP's phones that are down, as every page's banner and a notification say: one offline for
// ten minutes or more, or one that says it can't send texts.

/** How long a phone is offline before it's down. */
export const OFFLINE_MS = 10 * 60_000;

/** A phone that's down: how, what says it, and by its `key`, each time it went down. */
export type Down = { kind: 'offline' | 'cant_send'; key: string; words: string };

/** The `phones` an app holds that are down at `now`: by name while there are several. */
export function down(phones: readonly DispatchPhone[], now: number): Down[] {
  const named = (phone: DispatchPhone) => (phones.length > 1 ? phone.name : 'Dispatch phone');
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

/** The phones' card on DSP Connections, for whoever manages the DSP's phones or connections. */
export function phonesCard({ view }: Watching) {
  if (!can(view, 'connections.manage') && !can(view, 'phones.manage')) return undefined;
  return settingsHash(view.dsp.id, 'connections', view);
}
