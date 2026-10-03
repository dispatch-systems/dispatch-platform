import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';
import { useDeferredValue, useMemo, useState } from 'react';
import { RefreshCw, Plus, Ellipsis } from 'lucide-react';
import type { DspView, Membership, Role } from '../../../shared/contracts/index.js';
import { useUpdateState } from '../../../core/shell/frontend/runtime/browser-update.js';
import { api, useCachedData } from '../../../core/shell/frontend/runtime/api.js';
import {
  Badge,
  ConfirmDialog,
  DataState,
  DataTable,
  Empty,
  ErrorBox,
  Header,
  Modal,
  Popover,
  SearchInput,
  Tabs,
  TablePagination,
  useDataTable,
  type TableColumn,
} from '../../../core/shell/frontend/ui/index.js';
import { time } from '../../../core/shell/frontend/lib/format.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { RoleSheet } from './RoleSheet.js';
import { RolesTab } from './RolesTab.js';
import { assignable } from './assignable.js';
import { useMembers, inviteMember, setMemberRole, useRoles, removeRole } from '../api/client.js';

type Invitation = { email: string; role: string; expiresAt: number; accepted: boolean };
const actions = <span className="sr-only">Actions</span>;
export function TeamPage({ view, reopen }: { view: DspView; reopen: () => Promise<void> }) {
  const [tab, setTab] = useUpdateState('team-tab', 'members');
  const { data, error, refresh } = useMembers(
    tab === 'members' ? performancePolicy.teamPollMs : -1,
  );
  const canInvite = can(view, 'members.invite'),
    canManage = can(view, 'members.manage'),
    canRoles = can(view, 'roles.manage');
  const invitations = useCachedData<Invitation[]>(
    canInvite && tab === 'invitations' ? '/api/dsp/invitations' : '',
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
  const [search, setSearch] = useUpdateState('team-search', '');
  const [inviting, setInviting] = useState(false);
  const [editing, setEditing] = useState<Membership>();
  const [removing, setRemoving] = useState<Membership>();
  const [revoking, setRevoking] = useState<Invitation>();
  const revoke = useAction(
    async (invitation: Invitation) => {
      await api('/api/dsp/invitations/revoke', { email: invitation.email });
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
    { success: (form) => `Invitation email queued for ${form.get('email')}` },
  );
  // A null role removes the member.
  const assign = useAction(
    async (member: Membership, role: FormDataEntryValue | null) => {
      await setMemberRole(member.id, role === null ? null : String(role));
      setEditing(undefined);
      setRemoving(undefined);
      await reopen();
      refresh();
    },
    { success: (_, role) => (role === null ? 'Member removed' : 'Role updated') },
  );
  const searchable = useMemo(
    () =>
      data?.map((member) => ({
        member,
        text: `${member.name} ${member.email}`.toLowerCase(),
      })) ?? [],
    [data],
  );
  const searchText = search.toLowerCase();
  const deferredSearch = useDeferredValue(searchText);
  const searching = searchText !== deferredSearch;
  const members = useMemo(
    () =>
      searchable.filter(({ text }) => text.includes(deferredSearch)).map(({ member }) => member),
    [searchable, deferredSearch],
  );
  const memberColumns: TableColumn<Membership>[] = [
    {
      id: 'member',
      header: 'Member',
      headerClassName: 'team-member-column',
      value: (member) => member.name,
      cell: (member) => (
        <div className="member-identity">
          <span className="avatar">
            {member.name
              .split(/\s+/)
              .slice(0, 2)
              .map((part) => part[0])
              .join('')}
          </span>
          <div>
            <strong>{member.name}</strong>
            <small>{member.email}</small>
          </div>
        </div>
      ),
    },
    { id: 'role', header: 'Role', value: (member) => member.role, cell: (member) => member.role },
    {
      id: 'status',
      header: 'Status',
      value: (member) => member.status,
      cell: (member) => <Badge value={member.status} />,
    },
    {
      id: 'actions',
      header: actions,
      className: 'cell-end',
      cell: (member) =>
        canManage &&
        grantable.some((role) => role.id === member.roleId) && (
          <Popover
            className="row-menu"
            label={`Actions for ${member.name}`}
            trigger={<Ellipsis size={18} />}
            anchored
          >
            <button onClick={() => setEditing(member)}>Change role</button>
            <button className="danger" onClick={() => setRemoving(member)}>
              Remove member
            </button>
          </Popover>
        ),
    },
  ];
  const memberTable = useDataTable({
    columns: memberColumns,
    rows: members,
    rowId: (member) => member.id,
    pageSize: 100,
  });
  const pending = useMemo(
    () =>
      invitations.data?.filter(
        (invitation) => !invitation.accepted && invitation.expiresAt > Date.now(),
      ) ?? [],
    [invitations.data],
  );
  const invitationColumns: TableColumn<Invitation>[] = [
    {
      id: 'email',
      header: 'Email address',
      value: (invitation) => invitation.email,
      cell: (invitation) => invitation.email,
    },
    {
      id: 'role',
      header: 'Role',
      value: (invitation) => invitation.role,
      cell: (invitation) => invitation.role,
    },
    {
      id: 'expires',
      header: 'Expires',
      value: (invitation) => invitation.expiresAt,
      cell: (invitation) => time(new Date(invitation.expiresAt).toISOString(), view.dsp.timezone),
    },
    {
      id: 'actions',
      header: actions,
      cell: (invitation) => (
        <button
          className="text-button"
          aria-label={`Revoke invitation for ${invitation.email}`}
          onClick={() => setRevoking(invitation)}
        >
          Revoke
        </button>
      ),
    },
  ];
  const invitationTable = useDataTable({
    columns: invitationColumns,
    rows: pending,
    rowId: (invitation, index) => `${invitation.email}:${index}`,
    pageSize: 100,
  });
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
        onChange={setTab}
        items={[
          ['members', 'Members'],
          ['roles', 'Roles'],
          ...(canInvite ? [['invitations', 'Invitations']] : []),
        ]}
        label="Team"
      />
      <ErrorBox message={error || roles.error || invitations.error} />
      {tab === 'members' && (
        <>
          <div className="table-toolbar">
            <SearchInput
              label="Search members"
              placeholder="Search members"
              value={search}
              onChange={(value) => {
                setSearch(value);
                memberTable.setPage(0);
              }}
            />
            <button className="icon-button" aria-label="Refresh members" onClick={refresh}>
              <RefreshCw size={16} />
            </button>
          </div>
          <DataState data={data} failed={Boolean(error)} retry={refresh}>
            {() => (
              <div className="table-wrap" aria-busy={searching} inert={searching}>
                <DataTable table={memberTable} label="Team members" />
                <TablePagination table={memberTable} />
                {!members.length && (
                  <Empty title="No team members">
                    Invite a member to give them access to this DSP.
                  </Empty>
                )}
              </div>
            )}
          </DataState>
        </>
      )}
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
      {tab === 'invitations' && canInvite && (
        <>
          <div className="table-toolbar">
            <p className="muted">Pending invitations to your DSP.</p>
            <button
              className="icon-button"
              aria-label="Refresh invitations"
              onClick={invitations.refresh}
            >
              <RefreshCw size={16} />
            </button>
          </div>
          <DataState
            data={invitations.data}
            failed={Boolean(invitations.error)}
            retry={invitations.refresh}
          >
            {() => (
              <div className="table-wrap">
                <DataTable table={invitationTable} label="Pending invitations" />
                <TablePagination table={invitationTable} />
                {!pending.length && (
                  <Empty title="No invitations">Invite a team member to get started.</Empty>
                )}
              </div>
            )}
          </DataState>
        </>
      )}
      {revoking && (
        <ConfirmDialog
          title="Revoke invitation"
          confirm="Revoke invitation"
          onConfirm={() => void revoke.run(revoking)}
          onCancel={() => setRevoking(undefined)}
        >
          Revoke the pending invitation for {revoking.email}? Its link will stop working.
        </ConfirmDialog>
      )}
      {inviting && (
        <Modal
          variant="sheet"
          title="Invite member"
          description={`Give someone access to ${view.dsp.name}.`}
          onClose={() => setInviting(false)}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void invite.run(new FormData(event.currentTarget));
            }}
          >
            <label>
              Email address
              <input name="email" type="email" required />
            </label>
            <label>
              Role
              <select
                name="role"
                required
                defaultValue={
                  (grantable.find((role) => role.name === 'Member') ?? grantable.at(-1))?.id
                }
              >
                {grantable.map((role) => (
                  <option key={role.id} value={role.id}>
                    {role.name}
                  </option>
                ))}
              </select>
            </label>
            <div className="form-actions">
              <button type="button" onClick={() => setInviting(false)}>
                Cancel
              </button>
              <button className="primary">Send invitation</button>
            </div>
          </form>
        </Modal>
      )}
      {editing && (
        <Modal title={`Change role for ${editing.name}`} onClose={() => setEditing(undefined)}>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void assign.run(editing, new FormData(event.currentTarget).get('role'));
            }}
          >
            <label>
              Role
              <select name="role" defaultValue={editing.roleId ?? undefined}>
                {grantable.map((role) => (
                  <option key={role.id} value={role.id}>
                    {role.name}
                  </option>
                ))}
              </select>
            </label>
            <div className="form-actions">
              <button type="button" onClick={() => setEditing(undefined)}>
                Cancel
              </button>
              <button className="primary">Save role</button>
            </div>
          </form>
        </Modal>
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
        <ConfirmDialog
          title="Remove member"
          confirm="Remove member"
          tone="danger"
          busy={assign.busy}
          onConfirm={() => void assign.run(removing, null)}
          onCancel={() => setRemoving(undefined)}
        >
          Remove {removing.name} and delete their account?
        </ConfirmDialog>
      )}
    </>
  );
}
