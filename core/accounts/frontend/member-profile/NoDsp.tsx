import { MapPinOff } from 'lucide-react';
import { MemberProfileLayout } from './MemberProfileLayout.js';

/** A DSP's address that no DSP has: a short code mistyped, or one never given out. */
export function NoDsp() {
  return (
    <MemberProfileLayout title="No DSP here" route="cancelled">
      <p className="member-profile-notice">
        <MapPinOff size={19} aria-hidden="true" />
        <span>No DSP uses this address. Check the address your DSP gave you.</span>
      </p>
    </MemberProfileLayout>
  );
}
