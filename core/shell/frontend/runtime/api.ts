import { asksVerification, recoverAccess, verification } from './access-recovery.js';
import { performancePolicy } from '../lib/performance-policy.js';
import { beginBrowserWrite } from './browser-update.js';
import { clearNavigationState } from './navigation-state.js';
import { parseApiResponse } from './replies.js';
import { dataCache } from './data-cache.js';
import { clearDestinations } from './navigation.js';
import { mutationAffects } from './data-policy.js';
import { errorLabelOf, isLongPoll } from './slots.js';
import type { ReplyContext } from '../../../foundation/api/runtime.js';
let replyContext: ReplyContext = {};
export let csrf = '',
  view = '';
/** Whose view a view token is: its DSP, and the role viewed as, without its signature. */
export const viewing = (token: string) => token.slice(0, token.lastIndexOf('.') + 1);
export function credentials(nextCsrf: string, nextView = '', dsp?: ReplyContext['dsp']) {
  if (csrf && csrf !== nextCsrf) {
    clearNavigationState();
    clearDestinations();
  }
  if (
    csrf !== nextCsrf ||
    view !== nextView ||
    replyContext.dsp?.id !== dsp?.id ||
    replyContext.dsp?.code !== dsp?.code
  )
    dataCache.clear();
  csrf = nextCsrf;
  view = nextView;
  replyContext = nextView && dsp ? { dsp: { id: dsp.id, code: dsp.code } } : {};
}
export class ApiError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status: number,
    readonly requestId?: string,
  ) {
    super(message);
  }
}
const labels: Record<string, string> = {
  observation_read_only: 'This browser session has read-only access.',
  already_a_member: 'This person already has access. Change their role in the member list.',
  email_queue_full: 'Email capacity is temporarily full. Try again later.',
  mfa_required: 'Verify your identity to continue.',
  reauthentication_required: 'Verify your identity, then retry this action.',
  sign_in_again: 'Verify your password, then retry this action.',
  passkey_failed: 'Passkey verification failed. Try again with a registered passkey.',
  passkeys_unavailable: 'Passkeys are unavailable for this Dispatch origin.',
  passkey_unavailable: 'No passkey is available for this account.',
  passkey_exists: 'This passkey is already registered.',
  invalid_authenticator_code: 'That authenticator code is invalid, expired, or already used.',
  authenticator_exists: 'An authenticator app is already registered.',
  invalid_recovery_code: 'That recovery code is invalid or has already been used.',
  browser_update_required: 'Dispatch was updated. Refresh the page and try again.',
  schedule_changed: 'This schedule changed in another session. Reload it before saving.',
  settings_changed_reload_before_saving:
    'These settings changed in another session and have been reloaded. Make your change again.',
  schedule_not_found: 'This schedule has no rules. Refresh and try again.',
  schedule_limit: 'A DSP can have up to 50 schedule rules.',
  invalid_schedule_time: 'Choose valid times, each once.',
  invalid_schedule_interval: 'Choose an interval from 30 minutes to 12 hours, in half-hour steps.',
  invalid_schedule_days: 'Choose at least one day.',
  sync_in_progress: 'A collection is already in progress. Wait for it to finish, then sync again.',
  queue_full: 'The collection queue is full. Try again after the current collections finish.',
  invalid_date: 'Choose a valid date that is not in the future.',
  email_unavailable: 'Email sending is not configured for this environment.',
  invitation_expired: 'This invitation has expired or was revoked. Ask for a new invitation.',
  invalid_short_code: 'A short code is 2 to 16 letters and numbers.',
  short_code_taken: 'That short code is taken. Choose another.',
  short_code_locked: 'This DSP’s short code is already set.',
  invalid_login: 'The email or password is incorrect.',
  dsp_disabled: 'This DSP is disabled.',
  dsp_not_disabled: 'Disable this DSP first.',
  dsp_name_mismatch: 'The name doesn’t match.',
  dsp_deleting: 'This DSP is being deleted.',
  permission_denied: 'Your role does not allow this action.',
  dsp_view_expired: 'Your DSP access changed. Refreshing your view…',
  // Accepting an invitation whose role went raises it too.
  role_not_found: 'This role no longer exists. Refresh and try again.',
  connection_busy: 'The browser is busy. Please try again in a moment.',
  verification_expired: 'Verification expired. Check the connection to start again.',
  browser_capacity: 'Browser capacity is full. Try again shortly.',
  rate_limited: 'Too many attempts. Wait a few minutes and try again.',
  invalid_credentials: 'The provider could not verify those credentials.',
};
const recoveryCodeResponses = new Set([
  '/api/auth/security/passkeys/register/finish',
  '/api/auth/security/authenticator/register/finish',
  '/api/auth/security/recovery-codes',
]);
/** The verification prompt's own requests, which answer it rather than wait on it. */
const verifyingRequests = new Set([
  '/api/auth/security/reauthenticate',
  '/api/auth/security/recover',
  '/api/auth/security/authenticator/verify',
  '/api/auth/security/passkeys/verify/start',
  '/api/auth/security/passkeys/verify/finish',
]);
/** What an error code says: core's own, else as the owner that raises it words it. */
export function errorLabel(code: string): string | undefined {
  return labels[code] ?? errorLabelOf(code);
}
/** Response failure defaults; wording and access recovery stay with the caller. */
export function responseError(
  status: number,
  code: string | undefined,
  message: string | undefined,
  requestId?: string,
) {
  return new ApiError(
    code ?? 'request_failed',
    message ?? 'The request could not be completed.',
    status,
    requestId,
  );
}
/**
 * `api`, with the failures `errors` names worded as it words them: for an owner whose error codes
 * only its own screens raise, so their wording loads with those screens rather than with the
 * manifest every page loads.
 */
export const wordedApi =
  (errors: Readonly<Record<string, string>>) =>
  async <T>(...call: Parameters<typeof api>) => {
    try {
      return await api<T>(...call);
    } catch (error) {
      const worded = error instanceof ApiError && errors[error.code];
      throw worded ? new ApiError(error.code, worded, error.status, error.requestId) : error;
    }
  };
/** A request refused until the person confirms it's them goes through once they have. */
export async function api<T>(
  url: string,
  body?: unknown,
  signal?: AbortSignal,
  priority: 'auto' | 'high' | 'low' = 'auto',
): Promise<T> {
  try {
    return await request<T>(url, body, signal, priority);
  } catch (error) {
    if (
      error instanceof ApiError &&
      asksVerification(error.code) &&
      !verifyingRequests.has(url) &&
      (await verification())
    )
      return request<T>(url, body, signal, priority);
    throw error;
  }
}
async function request<T>(
  url: string,
  body: unknown,
  signal: AbortSignal | undefined,
  priority: 'auto' | 'high' | 'low',
): Promise<T> {
  const method = body === undefined ? 'GET' : 'POST';
  const finish = body === undefined ? undefined : beginBrowserWrite();
  const requestView = view;
  const requestContext = replyContext;
  try {
    const response = await fetch(url, {
      method,
      credentials: 'same-origin',
      priority,
      headers: {
        ...(body !== undefined ? { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf } : {}),
        ...(body !== undefined && recoveryCodeResponses.has(url)
          ? { 'X-Dispatch-Recovery-Code-Format': 'grouped-v1' }
          : {}),
        ...(view ? { 'X-Dispatch-View': view } : {}),
      },
      ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
      signal:
        // A read that waits for a change has no read timeout.
        body === undefined && !url.startsWith('/api/dsp/collection-updates') && !isLongPoll(url)
          ? AbortSignal.any([
              ...(signal ? [signal] : []),
              AbortSignal.timeout(performancePolicy.readTimeoutMs),
            ])
          : signal,
    });
    // A proxy in front of the server may answer an error with a page of its own.
    const value = (await response.json().catch(() => ({}))) as {
      error?: string;
      message?: string;
    };
    if (!response.ok) {
      recoverAccess(url, response.status, value.error);
      throw responseError(
        response.status,
        value.error,
        (value.error && errorLabel(value.error)) ?? value.message,
        response.headers.get('x-request-id') ?? undefined,
      );
    }
    try {
      const parsed = (await parseApiResponse(url, method, value, requestContext)) as T;
      if (body !== undefined && requestView === view)
        dataCache.invalidate((key) => mutationAffects(url, key));
      return parsed;
    } catch {
      throw new ApiError(
        'invalid_api_response',
        'The server returned an unexpected response. Refresh and try again.',
        502,
        response.headers.get('x-request-id') ?? undefined,
      );
    }
  } catch (error) {
    if (error instanceof DOMException && error.name === 'TimeoutError')
      throw new ApiError(
        'request_timeout',
        'The connection is taking too long. Please try again.',
        504,
      );
    throw error;
  } finally {
    finish?.();
  }
}
