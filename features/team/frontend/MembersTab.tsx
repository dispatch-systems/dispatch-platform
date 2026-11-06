import { useDeferredValue, useMemo } from 'react';
import { Ellipsis, RefreshCw } from 'lucide-react';
import type { Membership, Role } from '../../../core/accounts/api/index.js';
import { useUpdateState } from '../../../core/shell/frontend/runtime/navigation-state.js';
import {
  Badge,
  ConfirmDialog,
  DataState,
  DataTable,
  Empty,
  Modal,
  Popover,
  SearchInput,
  TablePagination,
  useDataTable,
  type TableColumn,
} from '../../../core/shell/frontend/ui/index.js';

const actions = <span className="sr-only">Actions</span>;

export function MembersTab({
  active,
  data,
  failed,
  refresh,
  canManage,
  grantable,
  edit,
  remove,
}: {
  active: boolean;
  data?: Membership[];
  failed: boolean;
  refresh: () => void;
  canManage: boolean;
  grantable: Role[];
  edit: (member: Membership) => void;
  remove: (member: Membership) => void;
}) {
  const [search, setSearch] = useUpdateState('team-search', '');
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
            <button onClick={() => edit(member)}>Change role</button>
            <button className="danger" onClick={() => remove(member)}>
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
  if (!active) return null;
  return (
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
      <DataState data={data} failed={failed} retry={refresh}>
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
  );
}

export function ChangeMemberRoleDialog({
  member,
  grantable,
  close,
  assign,
}: {
  member: Membership;
  grantable: Role[];
  close: () => void;
  assign: (role: FormDataEntryValue | null) => void;
}) {
  return (
    <Modal title={`Change role for ${member.name}`} onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          assign(new FormData(event.currentTarget).get('role'));
        }}
      >
        <label>
          Role
          <select name="role" defaultValue={member.roleId ?? undefined}>
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
          <button className="primary">Save role</button>
        </div>
      </form>
    </Modal>
  );
}

export function RemoveMemberDialog({
  member,
  busy,
  confirm,
  close,
}: {
  member: Membership;
  busy: boolean;
  confirm: () => void;
  close: () => void;
}) {
  return (
    <ConfirmDialog
      title="Remove member"
      confirm="Remove member"
      tone="danger"
      busy={busy}
      onConfirm={confirm}
      onCancel={close}
    >
      Remove {member.name} and delete their account?
    </ConfirmDialog>
  );
}
