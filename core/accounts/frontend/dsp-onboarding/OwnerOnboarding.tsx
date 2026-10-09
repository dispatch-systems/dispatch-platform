import { useCallback, useState } from 'react';
import { ArrowLeft, ArrowRight } from 'lucide-react';
import { api } from '../../../shell/frontend/runtime/api.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { navigate, signInHash } from '../../../shell/frontend/runtime/navigation.js';
import { site } from '../../../shell/frontend/runtime/site.js';
import { ErrorBox } from '../../../shell/frontend/ui/index.js';
import { signInAt } from '../sign-in-handoff.js';
import { DspSetupForm, type DspSetup } from './DspSetupForm.js';
import { OnboardingLayout } from './OnboardingLayout.js';

/**
 * A new DSP's first owner sets it up and makes their profile in one go, then signs in at the
 * address its short code gave it.
 */
export function OwnerOnboarding({ token, email }: { token: string; email: string }) {
  const [profile, setProfile] = useState<DspSetup>();
  const [step, setStep] = useState<1 | 2>(1);
  const [passwordError, setPasswordError] = useState('');
  const invitation = `/api/invitations/${encodeURIComponent(token)}`;
  const checkCode = useCallback(
    (code: string) =>
      api<{ available: boolean }>(`${invitation}/short-code?code=${encodeURIComponent(code)}`),
    [invitation],
  );
  const save = useAction(
    async (form: HTMLFormElement) => {
      const values = new FormData(form);
      const accepted = await api<{ email: string; signIn: string | null }>(`${invitation}/accept`, {
        firstName: String(values.get('firstName')).trim(),
        lastName: String(values.get('lastName')).trim(),
        password: String(values.get('password')),
        dspProfile: profile,
      });
      signInAt(accepted.email, accepted.signIn);
    },
    { inline: true },
  );
  // Nobody signs in at the invite page, so its setup has no way back to Sign In.
  const backToSignIn = site().kind === 'invite' ? undefined : () => navigate(signInHash);
  return (
    <OnboardingLayout title={step === 1 ? 'Set up your DSP' : 'Create your profile'} step={step}>
      <DspSetupForm
        hidden={step !== 1}
        checkCode={checkCode}
        onSubmit={(next) => {
          setProfile(next);
          setStep(2);
        }}
        onBack={backToSignIn}
      />
      <form
        hidden={step !== 2}
        aria-busy={save.busy}
        onSubmit={(event) => {
          event.preventDefault();
          const form = new FormData(event.currentTarget);
          if (form.get('password') !== form.get('confirmPassword')) {
            setPasswordError('The passwords must match.');
            return;
          }
          setPasswordError('');
          void save.run(event.currentTarget);
        }}
      >
        <div className="onboarding-fields">
          <label className="onboarding-wide">
            <span>Email address</span>
            <input type="email" autoComplete="email" readOnly value={email} />
          </label>
          <label>
            <span>First name</span>
            <input
              name="firstName"
              autoComplete="given-name"
              required
              maxLength={100}
              pattern=".*\S.*"
              disabled={save.busy}
            />
          </label>
          <label>
            <span>Last name</span>
            <input
              name="lastName"
              autoComplete="family-name"
              required
              maxLength={100}
              pattern=".*\S.*"
              disabled={save.busy}
            />
          </label>
          <label className="onboarding-wide">
            <span>Password</span>
            <input
              name="password"
              type="password"
              autoComplete="new-password"
              required
              minLength={15}
              maxLength={128}
              disabled={save.busy}
            />
          </label>
          <label className="onboarding-wide">
            <span>Confirm password</span>
            <input
              name="confirmPassword"
              type="password"
              autoComplete="new-password"
              required
              minLength={15}
              maxLength={128}
              disabled={save.busy}
            />
          </label>
        </div>
        <ErrorBox message={passwordError || save.error} />
        <div className="onboarding-actions">
          <button className="primary" disabled={save.busy}>
            {save.busy ? 'Please wait…' : 'Finish setup'}
            <ArrowRight size={21} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="onboarding-back"
            disabled={save.busy}
            onClick={() => setStep(1)}
          >
            <ArrowLeft size={15} aria-hidden="true" />
            Back to DSP setup
          </button>
        </div>
      </form>
    </OnboardingLayout>
  );
}
