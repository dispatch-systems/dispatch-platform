import { useMemo } from 'react';
import { RefreshCw } from 'lucide-react';
import type { DspInvitation, Role } from '../../../core/accounts/api/index.js';
import {
  ConfirmDialog,
  DataState,
  DataTable,
  Empty,
  Modal,
  TablePagination,
  useDataTable,
  type TableColumn,
} from '../../../core/shell/frontend/ui/index.js';
import { time } from '../../../core/shell/frontend/lib/format.js';

const actions = <span className="sr-only">Actions</span>;

export function InvitationsTab({
  active,
  data,
  failed,
  refresh,
  timezone,
  revoke,
}: {
  active: boolean;
  data?: DspInvitation[];
  failed: boolean;
  refresh: () => void;
  timezone: string;
  revoke: (invitation: DspInvitation) => void;
}) {
  const pending = useMemo(
    () =>
      data?.filter((invitation) => !invitation.accepted && invitation.expiresAt > Date.now()) ?? [],
    [data],
  );
  const invitationColumns: TableColumn<DspInvitation>[] = [
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
      cell: (invitation) => time(new Date(invitation.expiresAt).toISOString(), timezone),
    },
    {
      id: 'actions',
      header: actions,
      cell: (invitation) => (
        <button
          className="text-button"
          aria-label={`Revoke invitation for ${invitation.email}`}
          onClick={() => revoke(invitation)}
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
  if (!active) return null;
  return (
    <>
      <div className="table-toolbar">
        <p className="muted">Pending invitations to your DSP.</p>
        <button className="icon-button" aria-label="Refresh invitations" onClick={refresh}>
          <RefreshCw size={16} />
        </button>
      </div>
      <DataState data={data} failed={failed} retry={refresh}>
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
  );
}

export function RevokeInvitationDialog({
  invitation,
  confirm,
  close,
}: {
  invitation: DspInvitation;
  confirm: () => void;
  close: () => void;
}) {
  return (
    <ConfirmDialog
      title="Revoke invitation"
      confirm="Revoke invitation"
      onConfirm={confirm}
      onCancel={close}
    >
      Revoke the pending invitation for {invitation.email}? Its link will stop working.
    </ConfirmDialog>
  );
}

export function InviteMemberDialog({
  dspName,
  grantable,
  close,
  submit,
}: {
  dspName: string;
  grantable: Role[];
  close: () => void;
  submit: (form: FormData) => void;
}) {
  return (
    <Modal
      variant="sheet"
      title="Invite member"
      description={`Give someone access to ${dspName}.`}
      onClose={close}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          submit(new FormData(event.currentTarget));
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
          <button type="button" onClick={close}>
            Cancel
          </button>
          <button className="primary">Send invitation</button>
        </div>
      </form>
    </Modal>
  );
}
