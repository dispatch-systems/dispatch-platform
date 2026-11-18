import { MessageSquareOff, WifiOff } from 'lucide-react';
import type { Watching } from '../../../../core/shell/frontend/runtime/slots.js';
import { useNow } from '../page/useNow.js';
import { down, phonesCard } from './phones-down.js';
import { DISPATCH_PHONE, type TextingLine } from '../line.js';
import './phones-down.css';

/** How often what waits, the phones with it, is read again. */
const POLL_MS = 30_000;

// A banner atop every page while one of a line's phones is down, linking to its phones' card for
// whoever manages them. Mounted on every page for whoever uses the line, it reads what waits on
// it, with its phones, every 30 seconds, for the sidebar's count too, which follows what it read.

export function PhonesDown({
  line = DISPATCH_PHONE,
  ...access
}: Watching & { line?: TextingLine }) {
  const { data } = line.texting.useWaiting(POLL_MS);
  const now = useNow();
  const downs = down(data?.phones ?? [], now, line.phone);
  if (downs.length === 0) return null;
  const card = phonesCard(access, line);
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
