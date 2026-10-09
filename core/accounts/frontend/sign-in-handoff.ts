import { navigate, signInHash } from '../../shell/frontend/runtime/navigation.js';

type SignInHandoff = { email: string; animate: boolean };
let pending: SignInHandoff | undefined;

/** Ephemeral navigation data only; creating a profile never establishes a session. */
export function signInAfterProfile(email: string, animate = false) {
  pending = { email, animate };
  navigate(signInHash);
}

// Read without consuming during render, including React's repeated initial renders.
export const getSignInHandoff = () => pending;
export function clearSignInHandoff() {
  pending = undefined;
}

/**
 * Sign In where the account signs in: here, with its email filled in, or at its DSP's own
 * address when that is another, which opens its sign-in page.
 */
export function signInAt(email: string, signIn: string | null | undefined, animate = false) {
  if (signIn && new URL(signIn).origin !== location.origin) location.assign(signIn);
  else signInAfterProfile(email, animate);
}
