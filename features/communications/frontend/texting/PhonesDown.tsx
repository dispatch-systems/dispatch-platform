import { MessageSquareOff, WifiOff } from 'lucide-react';
import type { Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import { useCommunicationsWaiting } from '../../api/client.js';
import { useNow } from '../page/useNow.js';
import { down, phonesCard } from './phones-down.js';
import './phones-down.css';

/** How often what waits, the phones with it, is read again. */
const POLL_MS = 30_000;

// A banner atop every page while one of the DSP's phones is down, linking to the phones' card for
// whoever manages them. Mounted on every page for whoever has Communications, it reads what waits,
// with the phones, every 30 seconds, for the sidebar's count too, which follows what it read.

export function PhonesDown(access: Watching) {
  const { data } = useCommunicationsWaiting(POLL_MS);
  const now = useNow();
  const downs = down(data?.phones ?? [], now);
  if (downs.length === 0) return null;
  const card = phonesCard(access);
  return (
    <div className="phones-down" role="status">
      {downs.map(({ kind, key, words }) => {
        const said = (
          <>
            {kind === 'offline' ? (
              <WifiOff aria-hidden="true" />
            ) : (
              <MessageSquareOff aria-hidden="true" />
            )}
            {words}
          </>
        );
        return card ? (
          <a key={key} href={card}>
            {said}
          </a>
        ) : (
          <span key={key}>{said}</span>
        );
      })}
    </div>
  );
}
