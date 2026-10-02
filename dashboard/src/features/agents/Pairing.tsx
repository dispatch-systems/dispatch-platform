import { useEffect } from 'react';
import { Clock } from 'lucide-react';
import { openOAuthPairing, useOAuthPairing } from '../../app/endpoints.js';
import { useAction } from '../../app/useAction.js';
import { deviceTimezone, timeOfDay } from '../../lib/format.js';
import { ErrorBox } from '../../ui/index.js';

const allowLabel = 'Allow connecting for 10 minutes';
const openUntilText = (until: string) =>
  `Connecting is open until ${timeOfDay(until, deviceTimezone())}`;

/** The ten minutes in which an app may start connecting: until when they run, and a way to
 * start or extend them. */
export function usePairing() {
  const pairing = useOAuthPairing();
  const { refresh } = pairing;
  const allow = useAction(
    async () => {
      await openOAuthPairing();
      refresh();
    },
    { inline: true },
  );
  const until = pairing.data?.openUntil ?? null;
  // Read again once the window closes, so what the page says closes with it.
  useEffect(() => {
    if (!until) return;
    const timer = setTimeout(refresh, Math.max(1000, Date.parse(until) - Date.now() + 500));
    return () => clearTimeout(timer);
  }, [until, refresh]);
  return { until, open: () => void allow.run(), busy: allow.busy, error: allow.error };
}

/** Whether apps may start connecting now, and the button that lets them. */
export function PairingLine({ pairing }: { pairing: ReturnType<typeof usePairing> }) {
  return (
    <div className="agents-pairing">
      <button type="button" disabled={pairing.busy} onClick={pairing.open}>
        <Clock size={14} aria-hidden="true" />
        {allowLabel}
      </button>
      <span aria-live="polite">{pairing.until && openUntilText(pairing.until)}</span>
      <ErrorBox message={pairing.error} />
    </div>
  );
}
