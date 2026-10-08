import { useCachedData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { Membership, Permission, Role } from '../../../core/accounts/api/index.js';

// Team & Roles' endpoints, as its page calls them. Only its own screens raise these, so their
// words load with them.
const api = wordedApi({
  last_owner_required: 'Keep at least one DSP owner.',
  role_exceeds_permissions: 'You can only manage roles and members within your own permissions.',
  role_in_use: 'Move this role’s members to another role first.',
  role_name_taken: 'Another role already uses this name.',
  invalid_role_name: 'Choose a role name up to 40 characters. “Owner” is reserved.',
  role_limit: 'You can create up to 50 roles for this DSP.',
  owner_role_locked: 'The Owner role cannot be changed.',
});

export const useMembers = (poll = 0) => useCachedData<Membership[]>('/api/dsp/members', poll);
export const inviteMember = (email: unknown, role: unknown) =>
  api('/api/dsp/members/invite', { email, role });
/** A null role removes the member from the DSP. */
export const setMemberRole = (member: string, role: string | null) =>
  api(`/api/dsp/members/${member}`, { role });

export const revokeInvitation = (email: string) => api('/api/dsp/invitations/revoke', { email });

export const useRoles = (poll = 0) => useCachedData<Role[]>('/api/dsp/roles', poll);
export const saveTeamRole = (
  id: string | undefined,
  role: { name: string; permissions: Permission[] },
) => api<Role>(id ? `/api/dsp/roles/${id}` : '/api/dsp/roles', role);
export const removeRole = (id: string) => api(`/api/dsp/roles/${id}/remove`, {});
