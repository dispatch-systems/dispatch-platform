import { api, useCachedData } from '../../../core/shell/frontend/runtime/api.js';
import type { Membership, Permission, Role } from '../../../shared/contracts/index.js';

// Team & Roles' endpoints, as its page calls them.

export const useMembers = (poll = 0) => useCachedData<Membership[]>('/api/dsp/members', poll);
export const inviteMember = (email: unknown, role: unknown) =>
  api('/api/dsp/members/invite', { email, role });
/** A null role removes the member from the DSP. */
export const setMemberRole = (member: string, role: string | null) =>
  api(`/api/dsp/members/${member}`, { role });

export const useRoles = (poll = 0) => useCachedData<Role[]>('/api/dsp/roles', poll);
export const saveTeamRole = (
  id: string | undefined,
  role: { name: string; permissions: Permission[] },
) => api<Role>(id ? `/api/dsp/roles/${id}` : '/api/dsp/roles', role);
export const removeRole = (id: string) => api(`/api/dsp/roles/${id}/remove`, {});
