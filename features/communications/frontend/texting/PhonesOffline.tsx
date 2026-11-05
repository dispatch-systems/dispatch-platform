import { WifiOff } from 'lucide-react';
import type { DispatchPhone } from '../../api/index.js';
import { waited } from '../shared/format.js';
import './phones-offline.css';

// A chip for each of the DSP's dispatch phones that isn't online, with how long since it was last
// seen: by its name when the DSP has several. Nothing while every phone is online.

export function PhonesOffline({ phones, now }: { phones: readonly DispatchPhone[]; now: number }) {
  const offline = phones.filter((phone) => !phone.online);
  if (offline.length === 0) return null;
  return (
    <div className="phones-offline">
      {offline.map((phone) => {
        const since = phone.lastSeenAt !== null && waited(phone.lastSeenAt, now);
        return (
          <span key={phone.id} className="phone-offline" role="status">
            <WifiOff aria-hidden="true" />
            <span>
              {phones.length > 1 ? `${phone.name} offline` : 'Phone offline'}
              {since && <span aria-hidden="true"> · {since.label}</span>}
            </span>
            {since && <span className="sr-only">for {since.spoken}</span>}
          </span>
        );
      })}
    </div>
  );
}
