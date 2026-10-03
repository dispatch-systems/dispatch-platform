import { api, useCachedData, useData } from '../../shell/frontend/runtime/api.js';
import type {
  AgentActivityPage,
  AgentKey,
  AgentKeyCreated,
  AgentKeyRequest,
  AgentKeys,
  AgentKeysRevoked,
  AgentWhoami,
  AuditPage,
  DspFeatureReport,
  DspFeatures,
  MailMessage,
  OAuthAllowedApps,
  OAuthAppChoice,
  OAuthApproval,
  OAuthPairing,
  OAuthPairingOpened,
  OAuthRedirect,
  OAuthRequest,
  PlatformHealth,
} from '../../../shared/contracts/platform-owner.js';
import type { DspSummary } from '../../../shared/contracts/accounts.js';
import type { Feature } from '../../../shared/contracts/tenancy.js';
import type { Job } from '../../../shared/contracts/collection.js';

// The platform owner's endpoints, as its dashboard calls them: DSPs, Diagnostics, the audit
// log and the Agents page.

/** Whose calls the Activity tab lists, and whether only those Dispatch refused. */
export type AgentActivityFilter = { key: string; outcome: '' | 'refused' };

export const usePlatformDsps = (poll = 0) =>
  useCachedData<DspSummary[]>('/api/platform/dsps', poll);
const dspFeatures = (dspId: string) => `/api/platform/dsps/${dspId}/features`;
export const useDspFeatures = (dspId: string) =>
  useCachedData<DspFeatureReport>(dspFeatures(dspId));
/** Switches one feature and whatever depends on it; the answer lists every switch. */
export const setDspFeature = (dspId: string, feature: Feature, enabled: boolean) =>
  api<DspFeatures>(dspFeatures(dspId), { feature, enabled });
export const usePlatformMail = (poll = 0) =>
  useCachedData<MailMessage[]>('/api/platform/mail', poll);
/** Gives a failed message a fresh set of attempts, or drops it. */
export const retryMail = (id: string) => api(`/api/platform/mail/${id}/retry`, {});
export const discardMail = (id: string) => api(`/api/platform/mail/${id}/discard`, {});
export const usePlatformJobs = (poll = 0) => useCachedData<Job[]>('/api/platform/jobs', poll);
export const usePlatformHealth = (poll = 0) =>
  useData<PlatformHealth>('/api/platform/health', poll);
const audit = '/api/platform/audit';
export const useAuditPage = (query: URLSearchParams, limit: number) =>
  useData<AuditPage>(`${audit}?${query}&limit=${limit}`, 10000);
export const exportAudit = (query: URLSearchParams) =>
  api<AuditPage>(`${audit}/export`, Object.fromEntries(query));

const agents = '/api/platform/agents';
/** Every agent key, and the DSPs a key can be given. */
export const useAgentKeys = (poll = 0) => useData<AgentKeys>(agents, poll);
/** A new key. Its token comes back this once. */
export const createAgentKey = (request: AgentKeyRequest) =>
  api<AgentKeyCreated>(`${agents}/keys`, request);
export const updateAgentKey = (id: string, request: AgentKeyRequest) =>
  api<AgentKey>(`${agents}/keys/${encodeURIComponent(id)}`, request);
export const revokeAgentKey = (id: string) =>
  api<AgentKey>(`${agents}/keys/${encodeURIComponent(id)}/revoke`, {});
export const revokeAllAgentKeys = () => api<AgentKeysRevoked>(`${agents}/revoke-all`, {});
const oauthRequest = (id: string) => `/api/platform/oauth/requests/${encodeURIComponent(id)}`;
/** An app asking to connect with "Sign in with Dispatch"; an empty id reads nothing. */
export const useOAuthRequest = (id: string) => useData<OAuthRequest>(id ? oauthRequest(id) : '');
/** The same request, saying what approving it under `name` would replace. */
export const readOAuthRequest = (id: string, name: string) =>
  api<OAuthRequest>(`${oauthRequest(id)}?${new URLSearchParams({ name })}`);
/** The owner's answer. Either one sends the browser back to the app at `redirect`. */
export const approveOAuthRequest = (id: string, approval: OAuthApproval) =>
  api<OAuthRedirect>(`${oauthRequest(id)}/approve`, approval);
export const denyOAuthRequest = (id: string) => api<OAuthRedirect>(`${oauthRequest(id)}/deny`, {});
const pairing = '/api/platform/oauth/pairing';
/** Until when an app may start connecting; null while it may not. */
export const useOAuthPairing = () => useData<OAuthPairing>(pairing);
/** Lets apps start connecting for the next ten minutes. */
export const openOAuthPairing = () => api<OAuthPairingOpened>(pairing, {});
const oauthApps = '/api/platform/oauth/apps';
/** Which apps may connect at all. */
export const useOAuthApps = () => useData<OAuthAllowedApps>(oauthApps);
export const allowOAuthApp = (choice: OAuthAppChoice) => api<OAuthAllowedApps>(oauthApps, choice);
/** One page of the calls agents made, newest first; `next` reads the page after it. */
export const agentActivityUrl = (filter: AgentActivityFilter, before?: string) => {
  const query = new URLSearchParams();
  if (filter.key) query.set('key', filter.key);
  if (filter.outcome) query.set('outcome', filter.outcome);
  if (before) query.set('before', before);
  const text = query.toString();
  return `${agents}/activity${text ? `?${text}` : ''}`;
};
export const useAgentActivity = (filter: AgentActivityFilter) =>
  useData<AgentActivityPage>(agentActivityUrl(filter));
export const readAgentActivity = (filter: AgentActivityFilter, before: string) =>
  api<AgentActivityPage>(agentActivityUrl(filter, before));
/** Asks Dispatch who `token` belongs to, as an agent would: with the key alone, never the
 * dashboard's session, which the server would refuse beside a key. */
export async function agentWhoami(
  token: string,
): Promise<{ ok: true; value: AgentWhoami } | { ok: false; error: string }> {
  try {
    const response = await fetch('/api/v1/whoami', {
      credentials: 'omit',
      headers: { Authorization: `Bearer ${token}` },
    });
    // A proxy's error page is no JSON; say what the server answered rather than nothing.
    const value = await response.json().catch(() => undefined);
    if (response.ok && value) return { ok: true, value };
    return { ok: false, error: value?.error ? String(value.error) : `http_${response.status}` };
  } catch {
    return { ok: false, error: 'network_error' };
  }
}
