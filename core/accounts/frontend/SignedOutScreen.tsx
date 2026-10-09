import { useState } from 'react';
import { consumeHashToken } from '../../shell/frontend/runtime/navigation.js';
import { site } from '../../shell/frontend/runtime/site.js';
import { InvitationScreen } from './InvitationScreen.js';
import { NoDsp } from './member-profile/NoDsp.js';
import { NoInvitation } from './member-profile/NoInvitation.js';
import { SignInScreen } from './sign-in/SignInScreen.js';

/** Route entry only; each screen owns its form, layout and behavior. */
export function SignedOutScreen({ onLogin }: { onLogin: () => Promise<void> }) {
  const [invitation] = useState(() => consumeHashToken('invite'));
  if (invitation !== undefined) return <InvitationScreen key={invitation} token={invitation} />;
  // Nobody signs in at the invite page, nor at a DSP's address no DSP has.
  if (site().kind === 'invite') return <NoInvitation />;
  if (site().kind === 'dsp' && !site().dsp) return <NoDsp />;
  return <SignInScreen onLogin={onLogin} />;
}
