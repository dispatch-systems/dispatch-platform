/** The recovery events shared by ordinary requests and best-effort presence beats. */
export function recoverAccess(url: string, status: number, code?: string) {
  if (code === 'reauthentication_required' || code === 'sign_in_again')
    window.dispatchEvent(new Event('dispatch-reauthenticate'));
  if (code === 'mfa_required') window.dispatchEvent(new Event('dispatch-mfa-required'));
  if (status === 401 && url !== '/api/auth/login')
    window.dispatchEvent(new Event('dispatch-signed-out'));
  if (code === 'dsp_view_expired' && url !== '/api/session/dsp')
    window.dispatchEvent(new Event('dispatch-view-expired'));
}
