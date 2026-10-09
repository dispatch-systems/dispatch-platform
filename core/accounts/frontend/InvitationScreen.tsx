import { Brand } from '../../shell/frontend/runtime/Brand.js';
import { useData } from '../../shell/frontend/runtime/api.js';
import { Loading } from '../../shell/frontend/ui/index.js';
import { InvitationAccepted } from './member-profile/InvitationAccepted.js';
import { InvitationExpired } from './member-profile/InvitationExpired.js';
import { MemberProfileCreation } from './member-profile/MemberProfileCreation.js';
import { OwnerOnboarding } from './dsp-onboarding/OwnerOnboarding.js';
import './invitation.css';

type OpenInvitation = {
  accepted?: false;
  email: string;
  dspName: string;
  role: string;
  stationCode: string;
  timezone: string;
  onboarding: boolean;
  /** The short code the platform owner already gave the DSP, if any. */
  code: string | null;
};
type AcceptedInvitation = {
  accepted: true;
  email: string;
  dspName: string;
  role: string;
  signIn: string | null;
};

export function InvitationScreen({ token }: { token: string }) {
  const invitation = useData<OpenInvitation | AcceptedInvitation>(
    `/api/invitations/${encodeURIComponent(token)}`,
  );
  const data = invitation.data;
  if (!data && !invitation.error)
    return (
      <main className="invitation-loading">
        <Brand />
        <Loading />
      </main>
    );
  // A spent link gets its own page; any other failure keeps the form with its error.
  if (invitation.errorCode === 'invitation_expired') return <InvitationExpired />;
  if (data?.accepted)
    return (
      <InvitationAccepted
        email={data.email}
        dspName={data.dspName}
        role={data.role}
        signIn={data.signIn}
      />
    );
  // Choose the screen before loading its artwork; the invitation flows stay independent.
  if (data?.onboarding && token)
    return <OwnerOnboarding token={token} email={data.email} code={data.code} />;
  return (
    <MemberProfileCreation
      token={token}
      email={data?.email}
      dspName={data?.dspName}
      role={data?.role}
      stationCode={data?.stationCode}
      timezone={data?.timezone}
      invitationError={invitation.error}
    />
  );
}
