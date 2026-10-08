import { useEffect, useRef, useState } from 'react';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { Modal } from '../../../core/shell/frontend/ui/index.js';
import { addFiles, type PickerSetup } from '../api/client.js';
import { Tile } from './items.js';

// Adding files someone made directly in Google Drive. Google's picker opens in a window of its
// own, signed in to Google as the account that holds Documents, and tells this page what was
// picked on a channel only this origin reaches. Fixture mode has no picker of Google's: it
// lists its Drive's files here instead.

const CHANNEL = 'dispatch-documents-picker';

/** Adds files from Drive into `folder`, or the top, calling `added` once they're in. */
export function useFromDrive(
  picker: PickerSetup | null,
  folder: string | undefined,
  added: () => void,
) {
  const [choosing, setChoosing] = useState(false);
  const channel = useRef<BroadcastChannel | undefined>(undefined);
  useEffect(() => () => channel.current?.close(), []);
  const adding = useAction(
    async (files: string[]) => {
      await addFiles(files, folder);
      added();
    },
    { success: (files) => (files.length === 1 ? '1 file added' : `${files.length} files added`) },
  );
  function open() {
    if (!picker) return;
    if (!picker.google) return setChoosing(true);
    // The word the window says back, so only what it picked for this page is added.
    const word = crypto.randomUUID();
    const sent = new URLSearchParams({ ...picker.google, account: picker.account, word });
    window.open(`/api/documents/google/picker#${sent}`, CHANNEL, 'popup,width=1000,height=720');
    channel.current?.close();
    channel.current = new BroadcastChannel(CHANNEL);
    channel.current.onmessage = (event: MessageEvent<{ word?: string; files?: string[] }>) => {
      if (event.data.word !== word || !event.data.files?.length) return;
      channel.current?.close();
      void adding.run(event.data.files);
    };
  }
  const dialog = choosing && picker && (
    <FixturePicker
      picker={picker}
      busy={adding.busy}
      close={() => setChoosing(false)}
      pick={async (files) => {
        if (await adding.run(files)) setChoosing(false);
      }}
    />
  );
  return { open, dialog };
}

/** Fixture mode's stand-in for Google's picker: the files its Drive holds out of reach. */
function FixturePicker({
  picker,
  busy,
  close,
  pick,
}: {
  picker: PickerSetup;
  busy: boolean;
  close: () => void;
  pick: (files: string[]) => void;
}) {
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const toggle = (id: string, on: boolean) =>
    setChosen((all) => {
      const next = new Set(all);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  return (
    <Modal title="Add from Google Drive" onClose={close}>
      <p className="muted">
        Files in {picker.account}'s Google Drive that Documents can't reach yet, as Google's picker
        would show them.
      </p>
      {picker.madeInDrive.length ? (
        <div className="documents-starters documents-picked">
          {picker.madeInDrive.map((file) => (
            <label key={file.id} className={chosen.has(file.id) ? 'chosen' : undefined}>
              <input
                type="checkbox"
                checked={chosen.has(file.id)}
                onChange={(event) => toggle(file.id, event.target.checked)}
              />
              <Tile kind={file.kind} />
              {file.name}
            </label>
          ))}
        </div>
      ) : (
        <p>Every file in it is in Documents already.</p>
      )}
      <div className="form-actions">
        <button onClick={close}>Cancel</button>
        <button
          className="primary"
          disabled={busy || chosen.size === 0}
          onClick={() => pick([...chosen])}
        >
          Add {chosen.size === 1 ? '1 file' : `${chosen.size} files`}
        </button>
      </div>
    </Modal>
  );
}
