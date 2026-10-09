import { MailX } from 'lucide-react';
import { MemberProfileLayout } from './MemberProfileLayout.js';

/** The invite page, opened without an invitation's link. */
export function NoInvitation() {
  return (
    <MemberProfileLayout title="No invitation" route="cancelled">
      <p className="member-profile-notice">
        <MailX size={19} aria-hidden="true" />
        <span>You didn’t receive an invite.</span>
      </p>
    </MemberProfileLayout>
  );
}
