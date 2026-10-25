let open: { answer: Promise<boolean>; settle: (verified: boolean) => void } | undefined;

/** Whether a refusal asks the person to confirm it's them before the request goes through. */
export const asksVerification = (code?: string) =>
  code === 'reauthentication_required' || code === 'sign_in_again';

/**
 * Asks the person to confirm it's them, once however many refused requests wait on it: true once
 * they have, false if the prompt closes first.
 */
export function verification() {
  if (!open) {
    let settle = (_: boolean) => {};
    const answer = new Promise<boolean>((resolve) => (settle = resolve));
    open = { answer, settle };
    window.dispatchEvent(new Event('dispatch-reauthenticate'));
  }
  return open.answer;
}

/** Answers the open `verification()`, from its prompt. */
export function verified(done: boolean) {
  open?.settle(done);
  open = undefined;
}

/** The recovery events shared by ordinary requests and best-effort presence beats. */
export function recoverAccess(url: string, status: number, code?: string) {
  if (asksVerification(code)) void verification();
  if (code === 'mfa_required') window.dispatchEvent(new Event('dispatch-mfa-required'));
  if (status === 401 && url !== '/api/auth/login')
    window.dispatchEvent(new Event('dispatch-signed-out'));
  if (code === 'dsp_view_expired' && url !== '/api/session/dsp')
    window.dispatchEvent(new Event('dispatch-view-expired'));
}
