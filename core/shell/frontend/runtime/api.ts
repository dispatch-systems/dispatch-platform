import { performancePolicy } from '../lib/performance-policy.js';
import { beginBrowserWrite, clearNavigationState } from './browser-update.js';
import { useEffect, useState, useCallback, useRef, useSyncExternalStore } from 'react';
import { parseApiResponse } from '../../../../shared/contracts/runtime.js';
import { backoff } from '../lib/backoff.js';
import { dataCache } from './data-cache.js';
import { clearDestinations } from './navigation.js';
import { mutationAffects } from './data-policy.js';
import { useReadAvailability } from '../lib/read-availability.js';
import { errorLabelOf, isLongPoll } from './slots.js';
export let csrf = '',
  view = '';
export function credentials(nextCsrf: string, nextView = '') {
  if (csrf && csrf !== nextCsrf) {
    clearNavigationState();
    clearDestinations();
  }
  if (csrf !== nextCsrf || view !== nextView) dataCache.clear();
  csrf = nextCsrf;
  view = nextView;
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
  schedule_not_found: 'This schedule was deleted. Close the editor and refresh.',
  schedule_limit: 'You can create up to 50 schedules for this DSP.',
  invalid_schedule_time: 'Choose a valid collection time.',
  invalid_schedule_interval: 'Choose an interval from 0.5 to 24 hours in half-hour increments.',
  sync_in_progress: 'A collection is already in progress. Wait for it to finish, then sync again.',
  queue_full: 'The collection queue is full. Try again after the current collections finish.',
  invalid_date: 'Choose a valid date that is not in the future.',
  email_unavailable: 'Email sending is not configured for this environment.',
  invitation_expired: 'This invitation has expired or was revoked. Ask for a new invitation.',
  sign_in_with_existing_password: 'Use your existing Dispatch password to accept this invitation.',
  invalid_login: 'The email or password is incorrect.',
  permission_denied: 'Your role does not allow this action.',
  last_owner_required: 'Keep at least one DSP owner.',
  dsp_view_expired: 'Your DSP access changed. Refreshing your view…',
  role_exceeds_permissions: 'You can only manage roles and members within your own permissions.',
  role_in_use: 'Move this role’s members to another role first.',
  role_name_taken: 'Another role already uses this name.',
  invalid_role_name: 'Choose a role name up to 40 characters. “Owner” is reserved.',
  role_not_found: 'This role no longer exists. Refresh and try again.',
  role_limit: 'You can create up to 50 roles for this DSP.',
  owner_role_locked: 'The Owner role cannot be changed.',
  connection_busy: 'The browser is busy. Please try again in a moment.',
  verification_expired: 'Verification expired. Check the connection to start again.',
  browser_capacity: 'Browser capacity is full. Try again shortly.',
  rate_limited: 'Too many attempts. Wait a few minutes and try again.',
  invalid_credentials: 'The provider could not verify those credentials.',
  agent_key_name_taken: 'Another key already uses this name.',
  agent_key_limit: 'You can have up to 50 keys in use. Revoke one first.',
  agent_key_revoked: 'This key was revoked. Make a new one instead.',
  agent_key_not_found: 'This key no longer exists. Refresh the page.',
  invalid_expiry: 'Choose an expiry between tomorrow and five years from now.',
};
const recoveryCodeResponses = new Set([
  '/api/auth/security/passkeys/register/finish',
  '/api/auth/security/authenticator/register/finish',
  '/api/auth/security/recovery-codes',
]);
/** What an error code says: core's own, else as the owner that raises it words it. */
export function errorLabel(code: string): string | undefined {
  return labels[code] ?? errorLabelOf(code);
}
export async function api<T>(
  url: string,
  body?: unknown,
  signal?: AbortSignal,
  priority: 'auto' | 'high' | 'low' = 'auto',
): Promise<T> {
  const finish = body === undefined ? undefined : beginBrowserWrite();
  const requestView = view;
  try {
    const response = await fetch(url, {
      method: body === undefined ? 'GET' : 'POST',
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
    const value = await response.json();
    if (!response.ok) {
      if (['reauthentication_required', 'sign_in_again'].includes(value.error))
        window.dispatchEvent(new Event('dispatch-reauthenticate'));
      if (value.error === 'mfa_required') window.dispatchEvent(new Event('dispatch-mfa-required'));
      if (response.status === 401 && url !== '/api/auth/login')
        window.dispatchEvent(new Event('dispatch-signed-out'));
      if (value.error === 'dsp_view_expired' && url !== '/api/session/dsp')
        window.dispatchEvent(new Event('dispatch-view-expired'));
      throw new ApiError(
        value.error,
        errorLabel(value.error) ?? value.message ?? 'The request could not be completed.',
        response.status,
        response.headers.get('x-request-id') ?? undefined,
      );
    }
    try {
      const parsed = parseApiResponse(url, body === undefined ? 'GET' : 'POST', value) as T;
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
/** Opt in to bounded session memory and background revalidation. */
export function useCachedData<T>(
  url: string,
  poll = 0,
  refreshKey?: string | null,
  load?: (signal: AbortSignal) => Promise<T>,
) {
  return useData<T>(url, poll, refreshKey, url, true, load);
}

export function useData<T>(
  url: string,
  poll = 0,
  refreshKey?: string | null,
  dataScope?: string,
  cache = false,
  load?: (signal: AbortSignal) => Promise<T>,
) {
  const availability = useReadAvailability();
  const cached = useSyncExternalStore(
    useCallback(
      (listener) => (cache ? dataCache.subscribe(url, listener) : () => {}),
      [cache, url],
    ),
    useCallback(() => (cache ? dataCache.peek(url) : undefined), [cache, url]),
  );
  const generation = cached?.generation ?? 0;
  const session = dataCache.session;
  const scope = cache ? url : (dataScope ?? url);
  const reader = useRef<(force?: boolean) => Promise<void>>(undefined);
  const previous = useRef<{ url: string; refreshKey?: string | null; revision: number }>(undefined);
  const [result, setResult] = useState<{
      data: T;
      scope: string | undefined;
      session: number;
      refreshKey?: string | null;
    }>(),
    [failure, setFailure] = useState<{
      message: string;
      code: string;
      scope: string;
      session: number;
    }>(),
    [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((v) => v + 1), []);
  useEffect(() => {
    const changed = previous.current;
    const force =
      changed?.url === url && (changed.refreshKey !== refreshKey || changed.revision !== revision);
    previous.current = { url, refreshKey, revision };
    const controller = new AbortController();
    let active = true;
    setFailure(undefined);
    let reading = false;
    let failures = 0;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const read = async (force = false) => {
      if (!url || reading || document.hidden || !navigator.onLine) return;
      reading = true;
      try {
        const request = load ?? ((signal: AbortSignal) => api<T>(url, undefined, signal));
        const value = cache
          ? await dataCache.read(url, request, force)
          : await request(controller.signal);
        if (
          active &&
          (!cache ||
            (generation === dataCache.peek(url).generation && session === dataCache.session))
        ) {
          setResult((previous) =>
            previous?.data === value &&
            previous.scope === scope &&
            previous.session === session &&
            previous.refreshKey === refreshKey
              ? previous
              : { data: value, scope, session, refreshKey },
          );
          setFailure(undefined);
          failures = 0;
          if (retry) clearTimeout(retry);
        }
      } catch (error) {
        if (
          active &&
          (!cache ||
            (generation === dataCache.peek(url).generation && session === dataCache.session)) &&
          error instanceof Error &&
          error.name !== 'AbortError'
        ) {
          setFailure({
            message: error.message,
            code: error instanceof ApiError ? error.code : '',
            scope,
            session,
          });
          // A missed table response must recover even when no further driver arrives.
          if (!(error instanceof ApiError) || error.status >= 500 || error.status === 429) {
            if (retry) clearTimeout(retry);
            retry = setTimeout(() => void read(true), backoff(failures++));
          }
        }
      } finally {
        reading = false;
      }
    };
    reader.current = read;
    const visible = () => {
      if (!document.hidden) void read(true);
    };
    document.addEventListener('visibilitychange', visible);
    window.addEventListener('online', visible);
    void read(force);
    return () => {
      active = false;
      controller.abort();
      document.removeEventListener('visibilitychange', visible);
      window.removeEventListener('online', visible);
      if (reader.current === read) reader.current = undefined;
      if (retry) clearTimeout(retry);
    };
  }, [url, revision, refreshKey, scope, cache, generation, session, load]);
  // Changing the active/idle cadence must never abort an in-progress read.
  useEffect(() => {
    const interval = poll < 0 ? 0 : poll || (cache ? performancePolicy.recoveryPollMs : 0);
    if (!interval) return;
    const timer = setInterval(() => void reader.current?.(true), interval);
    return () => clearInterval(timer);
  }, [poll, cache]);
  // A date-scoped view can keep its controls mounted without showing the previous day's rows.
  const shown = result?.session === session ? result : undefined;
  const data =
    (cached?.data as T | undefined) ?? (shown?.scope === scope ? shown?.data : undefined);
  const currentFailure =
    failure?.scope === scope && failure.session === session ? failure : undefined;
  const error = currentFailure?.message ?? '';
  const errorCode = currentFailure?.code ?? '';
  // The previous scope's value lets a view hold its layout, marked busy, until the new one lands.
  const stale = data || error ? undefined : shown?.data;
  const paused = Boolean(url) && availability !== 'ready';
  return {
    data,
    stale,
    error,
    errorCode,
    refresh,
    paused,
    loading: Boolean(url) && data === undefined && !error && !paused,
    validatedKey: shown?.refreshKey,
  };
}
