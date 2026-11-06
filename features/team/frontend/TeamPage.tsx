import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';
import { useState } from 'react';
import { Plus } from 'lucide-react';
import type { DspInvitation, DspView, Membership, Role } from '../../../core/accounts/api/index.js';
import { useUpdateState } from '../../../core/shell/frontend/runtime/browser-update.js';
import { HostedTab, usePageTabs } from '../../../core/shell/frontend/runtime/PageTabs.js';
import { ConfirmDialog, ErrorBox, Header, Tabs } from '../../../core/shell/frontend/ui/index.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { RoleSheet } from './RoleSheet.js';
import { RolesTab } from './RolesTab.js';
import { MembersTab, ChangeMemberRoleDialog, RemoveMemberDialog } from './MembersTab.js';
import { InvitationsTab, RevokeInvitationDialog, InviteMemberDialog } from './InvitationsTab.js';
import { assignable } from './assignable.js';
import {
  useMembers,
  useInvitations,
  inviteMember,
  revokeInvitation,
  setMemberRole,
  useRoles,
  removeRole,
} from '../api/client.js';
import { field } from '../../../core/shell/frontend/lib/form.js';

export function TeamPage({ view, reopen }: { view: DspView; reopen: () => Promise<void> }) {
  const [ownTab, setTab] = useUpdateState('team-tab', 'members');
  const canInvite = can(view, 'members.invite'),
    canManage = can(view, 'members.manage'),
    canRoles = can(view, 'roles.manage');
  const tabs = usePageTabs('team', [
    ['members', 'Members'],
    ['roles', 'Roles'],
    ...(canInvite ? [['invitations', 'Invitations'] as const] : []),
  ]);
  const tab = tabs.open?.id ?? ownTab;
  const { data, error, refresh } = useMembers(
    tab === 'members' ? performancePolicy.teamPollMs : -1,
  );
  const invitations = useInvitations(
    canInvite && tab === 'invitations',
    performancePolicy.teamPollMs,
  );
  const roles = useRoles(tab === 'roles' ? performancePolicy.teamPollMs : -1);
  const [roleEditor, setRoleEditor] = useState<Role | 'new'>();
  const [removingRole, setRemovingRole] = useState<Role>();
  const deleteRole = useAction(
    async (role: Role) => {
      await removeRole(role.id);
      setRemovingRole(undefined);
      roles.refresh();
      refresh();
    },
    { success: 'Role deleted' },
  );
  const grantable = roles.data?.filter((role) => assignable(view, role, roles.data ?? [])) ?? [];
  // Dialogs and in-flight actions outlive tab changes; success coordinates the shared reads here.
  const [inviting, setInviting] = useState(false);
  const [editing, setEditing] = useState<Membership>();
  const [removing, setRemoving] = useState<Membership>();
  const [revoking, setRevoking] = useState<DspInvitation>();
  const revoke = useAction(
    async (invitation: DspInvitation) => {
      await revokeInvitation(invitation.email);
      setRevoking(undefined);
      invitations.refresh();
    },
    { success: 'Invitation revoked' },
  );
  const invite = useAction(
    async (form: FormData) => {
      await inviteMember(form.get('email'), form.get('role'));
      setInviting(false);
      invitations.refresh();
      roles.refresh();
    },
    { success: (form) => `Invitation email queued for ${field(form, 'email')}` },
  );
  // A null role removes the member.
  const assign = useAction(
    async (member: Membership, role: FormDataEntryValue | null) => {
      await setMemberRole(member.id, typeof role === 'string' ? role : null);
      setEditing(undefined);
      setRemoving(undefined);
      await reopen();
      refresh();
    },
    { success: (_, role) => (role === null ? 'Member removed' : 'Role updated') },
  );
  return (
    <>
      <Header title="Team & Roles">
        {tab === 'roles' && canRoles ? (
          <button className="primary" onClick={() => setRoleEditor('new')}>
            <Plus size={16} />
            Create role
          </button>
        ) : (
          canInvite && (
            <button className="primary" onClick={() => setInviting(true)}>
              <Plus size={16} />
              Invite member
            </button>
          )
        )}
      </Header>
      <Tabs
        value={tab}
        onChange={(id) => tabs.change(id, setTab)}
        onIntent={tabs.warm}
        items={tabs.items}
        label="Team"
      />
      {tabs.open && <HostedTab tab={tabs.open} />}
      <ErrorBox message={error || roles.error || invitations.error} />
      {/* Keep both views mounted so own and hosted tab changes retain search and table state. */}
      <MembersTab
        active={tab === 'members'}
        data={data}
        failed={Boolean(error)}
        refresh={refresh}
        canManage={canManage}
        grantable={grantable}
        edit={setEditing}
        remove={setRemoving}
      />
      {tab === 'roles' && (
        <RolesTab
          view={view}
          roles={roles.data}
          failed={Boolean(roles.error)}
          retry={roles.refresh}
          edit={setRoleEditor}
          remove={setRemovingRole}
        />
      )}
      {roleEditor && (
        <RoleSheet
          view={view}
          role={roleEditor === 'new' ? undefined : roleEditor}
          close={() => setRoleEditor(undefined)}
          saved={async (permissionsChanged) => {
            if (permissionsChanged) await reopen();
            roles.refresh();
            refresh();
          }}
        />
      )}
      <InvitationsTab
        active={tab === 'invitations' && canInvite}
        data={invitations.data}
        failed={Boolean(invitations.error)}
        refresh={invitations.refresh}
        timezone={view.dsp.timezone}
        revoke={setRevoking}
      />
      {revoking && (
        <RevokeInvitationDialog
          invitation={revoking}
          confirm={() => void revoke.run(revoking)}
          close={() => setRevoking(undefined)}
        />
      )}
      {inviting && (
        <InviteMemberDialog
          dspName={view.dsp.name}
          grantable={grantable}
          close={() => setInviting(false)}
          submit={(form) => void invite.run(form)}
        />
      )}
      {editing && (
        <ChangeMemberRoleDialog
          member={editing}
          grantable={grantable}
          close={() => setEditing(undefined)}
          assign={(role) => void assign.run(editing, role)}
        />
      )}
      {removingRole && (
        <ConfirmDialog
          title="Delete role"
          confirm="Delete role"
          tone="danger"
          busy={deleteRole.busy}
          onConfirm={() => void deleteRole.run(removingRole)}
          onCancel={() => setRemovingRole(undefined)}
        >
          Delete {removingRole.name}?
          {Boolean(removingRole.invitations) &&
            ` This also cancels ${removingRole.invitations} pending ${
              removingRole.invitations === 1 ? 'invitation' : 'invitations'
            }.`}
        </ConfirmDialog>
      )}
      {removing && (
        <RemoveMemberDialog
          member={removing}
          busy={assign.busy}
          confirm={() => void assign.run(removing, null)}
          close={() => setRemoving(undefined)}
        />
      )}
    </>
  );
}
