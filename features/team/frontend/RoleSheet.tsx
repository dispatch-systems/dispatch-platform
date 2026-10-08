import { useState } from 'react';
import {
  permissions as allPermissions,
  type DspView,
  type Permission,
  type Role,
} from '../../../core/accounts/api/index.js';
import { Modal } from '../../../core/shell/frontend/ui/index.js';
import {
  can,
  impliedPermissions as implied,
  permissionLabels,
  permissionParents,
  visiblePermissionGroups,
  visiblePermissions,
} from '../../../core/shell/frontend/runtime/permissions.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { saveTeamRole } from '../api/client.js';

export function RoleSheet({
  view,
  role,
  close,
  saved,
}: {
  view: DspView;
  role?: Role;
  close: () => void;
  saved: (permissionsChanged: boolean) => Promise<void> | void;
}) {
  const [name, setName] = useState(role?.name ?? '');
  // Permissions of features the DSP lacks are never shown; the server keeps them as they are.
  const shown = visiblePermissions(view, role?.permissions ?? []);
  const groups = visiblePermissionGroups(view);
  const [chosen, setChosen] = useState<Permission[]>(shown);
  const [confirming, setConfirming] = useState(false);
  const ordered = (permissions: Permission[]) =>
    allPermissions.filter((p) => permissions.includes(p));
  const dirty = name !== (role?.name ?? '') || ordered(chosen).join() !== ordered(shown).join();
  // Edits leave only through Save or Discard; closing the tab drops them silently.
  const leave = () => (dirty ? setConfirming(true) : close());
  // What a chosen permission includes can't be taken away on its own.
  const includer = (permission: Permission) =>
    allPermissions.find((p) => chosen.includes(p) && implied[p]?.includes(permission));
  const locked = (permission: Permission) => includer(permission) !== undefined;
  const save = useAction(
    async () => {
      await saveTeamRole(role?.id, {
        name,
        permissions: allPermissions.filter((p) => chosen.includes(p)),
      });
      close();
      await saved(Boolean(role));
    },
    { success: role ? 'Role updated' : 'Role created' },
  );
  function toggle(permission: Permission, on: boolean) {
    setChosen((current) => {
      const next = current.filter((p) => p !== permission);
      if (!on) return next;
      const needs = (implied[permission] ?? []).filter((p) => !next.includes(p));
      return [...next, permission, ...needs];
    });
  }
  return (
    <Modal variant="sheet" title={role ? `Edit ${role.name}` : 'Create role'} onClose={leave}>
      <form
        className="role-form"
        onSubmit={(event) => {
          event.preventDefault();
          void save.run();
        }}
      >
        <label>
          Role name
          <input
            name="name"
            required
            maxLength={40}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {groups.map(([group, items]) => (
          <fieldset className="permission-group" key={group}>
            <legend>{group}</legend>
            <div className="permission-rows">
              {items.map((permission) => (
                <label
                  className={`permission-row${permissionParents[permission] ? ' nested' : ''}`}
                  key={permission}
                >
                  <span>
                    {permissionLabels[permission]}
                    {locked(permission) && (
                      <small>Included with {permissionLabels[includer(permission)!]}</small>
                    )}
                  </span>
                  <input
                    type="checkbox"
                    role="switch"
                    checked={chosen.includes(permission)}
                    disabled={locked(permission) || !can(view, permission)}
                    onChange={(event) => toggle(permission, event.target.checked)}
                  />
                </label>
              ))}
            </div>
          </fieldset>
        ))}
        <div className="form-actions">
          <button type="button" onClick={leave}>
            Cancel
          </button>
          <button className="primary">{role ? 'Save role' : 'Create role'}</button>
        </div>
      </form>
      {confirming && (
        <Modal title="Save changes?" dismissible={false} onClose={() => {}}>
          <p>This role has unsaved changes.</p>
          <div className="form-actions">
            <button type="button" onClick={close}>
              Discard changes
            </button>
            <button
              type="button"
              className="primary"
              disabled={save.busy || !name.trim()}
              onClick={async () => {
                if (!(await save.run())) setConfirming(false);
              }}
            >
              Save changes
            </button>
          </div>
        </Modal>
      )}
    </Modal>
  );
}
