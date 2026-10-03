import { useEffect, useState } from 'react';
import { openOAuthPairing, useOAuthPairing } from '../../app/endpoints.js';
import { useAction } from '../../app/useAction.js';

const later = (a: string | null, b: string | null) =>
  a && b ? (Date.parse(a) >= Date.parse(b) ? a : b) : (a ?? b);

/** The ten minutes in which an app may start connecting: until when they run, and a way to
 * start or extend them. The window only ever closes when its time runs out. */
export function usePairing() {
  const pairing = useOAuthPairing();
  const { refresh } = pairing;
  // Until when opening it here said, which stands before the window is next read.
  const [opened, setOpened] = useState<string | null>(null);
  const allow = useAction(
    async () => {
      setOpened((await openOAuthPairing()).openUntil);
      refresh();
    },
    { inline: true },
  );
  const until = later(pairing.data?.openUntil ?? null, opened);
  // Read again once the window closes, so what the page says closes with it.
  useEffect(() => {
    if (!until) return;
    const timer = setTimeout(refresh, Math.max(1000, Date.parse(until) - Date.now() + 500));
    return () => clearTimeout(timer);
  }, [until, refresh]);
  return { until, open: () => void allow.run(), busy: allow.busy, error: allow.error };
}
