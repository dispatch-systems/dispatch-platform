import { api } from '../../../shell/frontend/runtime/api.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { DspSetupForm, type DspSetup } from './DspSetupForm.js';
import { OnboardingLayout } from './OnboardingLayout.js';

/**
 * Resume setup for owners who already created their account before the new flow, saving the
 * profile to the route the owner that saves it names.
 */
export function DspOnboarding({
  saveTo,
  code,
  complete,
  signOut,
}: {
  saveTo: string;
  /** The DSP's short code, when it has one already: then it stays. */
  code?: string | null;
  complete: () => Promise<void>;
  signOut: () => Promise<void>;
}) {
  const save = useAction((profile: DspSetup) => api(saveTo, profile).then(complete), {
    inline: true,
  });
  const logout = useAction(signOut, { inline: true });
  return (
    <OnboardingLayout title="Set up your DSP">
      <DspSetupForm
        resume
        locked={Boolean(code)}
        initial={code ? { abbreviation: code.toUpperCase() } : undefined}
        busy={save.busy || logout.busy}
        error={save.error || logout.error}
        onSubmit={(profile) => {
          void save.run(profile);
        }}
        onBack={() => {
          void logout.run();
        }}
      />
    </OnboardingLayout>
  );
}
