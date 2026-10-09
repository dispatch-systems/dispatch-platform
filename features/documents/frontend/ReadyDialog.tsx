import { useState } from 'react';
import { CircleCheck, Folder } from 'lucide-react';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox, Modal } from '../../../core/shell/frontend/ui/index.js';
import { createItem, type DocumentsConnection } from '../api/client.js';

/** Folders most DSPs keep, offered when Documents is new. */
const STARTERS: [string, boolean][] = [
  ['Safety & Compliance', true],
  ['Fleet', true],
  ['Drivers', true],
  ['HR & Payroll', true],
  ['Training', true],
  ['Templates', false],
];

/**
 * Right after Google is connected and Dispatch made the DSP's folder, with nothing in it yet:
 * some folders to start with.
 */
export function ReadyDialog({
  connection,
  close,
  made,
}: {
  connection: DocumentsConnection;
  close: () => void;
  made: () => void;
}) {
  const [chosen, setChosen] = useState(
    () => new Set(STARTERS.filter(([, on]) => on).map(([name]) => name)),
  );
  const adding = useAction(
    async () => {
      for (const [name] of STARTERS)
        if (chosen.has(name)) await createItem(undefined, 'folder', name);
      made();
    },
    { success: 'Folders added', inline: true },
  );
  const toggle = (name: string, on: boolean) =>
    setChosen((current) => {
      const next = new Set(current);
      if (on) next.add(name);
      else next.delete(name);
      return next;
    });
  return (
    <Modal
      title={
        <span className="documents-ready-title">
          <CircleCheck size={22} />
          Documents is ready
        </span>
      }
      onClose={close}
    >
      <p>
        Dispatch made <strong>{connection.folderName}</strong> in {connection.accountEmail}'s Google
        Drive.
      </p>
      <p className="muted documents-ready-note">
        Everyone who uses Documents gets it shared with them in a minute. Anyone whose email isn't a
        Google account is asked to link one.
      </p>
      <h3 className="documents-ready-heading">Start with some folders?</h3>
      <p className="muted">You can rename or delete them later.</p>
      <div className="documents-starters">
        {STARTERS.map(([name]) => (
          <label key={name} className={chosen.has(name) ? 'chosen' : undefined}>
            <input
              type="checkbox"
              checked={chosen.has(name)}
              onChange={(event) => toggle(name, event.target.checked)}
            />
            <Folder size={16} />
            {name}
          </label>
        ))}
      </div>
      <ErrorBox message={adding.error} />
      <div className="form-actions">
        <button onClick={close}>Skip</button>
        <button
          className="primary"
          disabled={adding.busy || chosen.size === 0}
          onClick={() => void adding.run()}
        >
          Add {chosen.size} folder{chosen.size === 1 ? '' : 's'}
        </button>
      </div>
    </Modal>
  );
}
