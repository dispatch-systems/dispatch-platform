import { WifiOff } from 'lucide-react';
import type { DispatchPhone } from '../../api/index.js';
import { waited } from '../format.js';

// A chip in the page's heading for each of the DSP's dispatch phones that isn't online, with how
// long since it was last seen: by its name when the DSP has several.

export function PhonesOffline({ phones, now }: { phones: readonly DispatchPhone[]; now: number }) {
  return phones
    .filter((phone) => !phone.online)
    .map((phone) => (
      <span key={phone.id} className="phone-offline" role="status">
        <WifiOff aria-hidden="true" />
        {phones.length > 1 ? `${phone.name} offline` : 'Phone offline'}
        {phone.lastSeenAt !== null && ` · ${waited(phone.lastSeenAt, now).label}`}
      </span>
    ));
}
