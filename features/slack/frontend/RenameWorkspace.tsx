import { useState } from 'react';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { Modal } from '../../../core/shell/frontend/ui/index.js';
import { renameSlack, type SlackWorkspace } from '../api/client.js';

// Renaming one of the DSP's workspaces in Dispatch. Left blank, it goes by its name in Slack.

export function RenameWorkspace({
  workspace,
  done,
  close,
}: {
  workspace: SlackWorkspace;
  /** Once it's renamed. */
  done: () => void;
  close: () => void;
}) {
  const [label, setLabel] = useState(workspace.label ?? '');
  const rename = useAction(
    async () => {
      await renameSlack(workspace.teamId, label.trim());
      done();
    },
    { success: 'Connection renamed' },
  );
  return (
    <Modal title="Rename connection" onClose={close} initialFocus="input">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void rename.run();
        }}
      >
        <label>
          Name
          <input
            value={label}
            placeholder={workspace.name}
            maxLength={60}
            onChange={(event) => setLabel(event.target.value)}
          />
        </label>
        <div className="form-actions">
          <button type="button" onClick={close}>
            Cancel
          </button>
          <button className="primary" disabled={rename.busy}>
            Save
          </button>
        </div>
      </form>
    </Modal>
  );
}
