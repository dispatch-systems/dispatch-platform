import { useData } from '../../shell/frontend/runtime/api.js';
import type { AccountSession, PasskeySummary, SecurityStatus } from './index.js';

// The account's endpoints, as its Settings panels call them.

export const useSecurityStatus = () => useData<SecurityStatus>('/api/auth/security/status');
export const usePasskeys = () => useData<PasskeySummary[]>('/api/auth/security/passkeys');
export const useAccountSessions = () => useData<AccountSession[]>('/api/auth/security/sessions');
