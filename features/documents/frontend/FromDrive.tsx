import { useEffect, useRef, useState } from 'react';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox, Modal } from '../../../core/shell/frontend/ui/index.js';
import { addFiles, type PickerSetup } from '../api/client.js';
import { driveLinks } from './driveLinks.js';
import { Tile } from './items.js';

// Adding files and folders someone made directly in Google Drive. Google's picker opens in a
// window of its own, signed in to Google as the account that holds Documents, on the folder
// open here, and tells this page what was picked on a channel only this origin reaches. A
// Google link, given in a dialog or pasted on the page, opens it on the files the link names.
// Fixture mode has no picker of Google's: it lists its Drive's files here instead.

const CHANNEL = 'dispatch-documents-picker';

/** Where the picker opens: on the files a link named, or a folder's, or else the open one's. */
type At = { files: string[]; folder?: string };

/** Adds files from Drive into `folder`, or the top, calling `added` once they're in. */
export function useFromDrive(
  picker: PickerSetup | null,
  folder: string | undefined,
  added: () => void,
) {
  // Fixture mode's list, with the files a pasted link named chosen.
  const [choosing, setChoosing] = useState<string[]>();
  // Where the picker was to open when the browser blocked its window.
  const [blocked, setBlocked] = useState<At>();
  const [linking, setLinking] = useState(false);
  const channel = useRef<BroadcastChannel | undefined>(undefined);
  useEffect(() => () => channel.current?.close(), []);
  const adding = useAction(
    async (files: string[]) => {
      await addFiles(files, folder);
      added();
    },
    { success: 'Added to Documents' },
  );
  function open(at: At = { files: [] }) {
    if (!picker) return;
    setBlocked(undefined);
    if (!picker.google) return setChoosing(at.files);
    // The word the window says back, so only what it picked for this page is added.
    const word = crypto.randomUUID();
    const sent = new URLSearchParams({ ...picker.google, account: picker.account, word });
    if (at.files.length) sent.set('files', at.files.join(','));
    else sent.set('folder', at.folder ?? folder ?? picker.folder);
    const opened = window.open(
      `/api/documents/google/picker#${sent}`,
      CHANNEL,
      'popup,width=1000,height=720',
    );
    if (!opened) return setBlocked(at);
    channel.current?.close();
    channel.current = new BroadcastChannel(CHANNEL);
    channel.current.onmessage = (event: MessageEvent<{ word?: string; files?: string[] }>) => {
      if (event.data.word !== word || !event.data.files?.length) return;
      channel.current?.close();
      void adding.run(event.data.files);
    };
  }
  // A Google link pasted anywhere but a field opens the picker on what it names.
  const opener = useRef(open);
  useEffect(() => {
    opener.current = open;
  });
  useEffect(() => {
    if (!picker) return;
    const paste = (event: ClipboardEvent) => {
      if ((event.target as Element | null)?.closest?.('input, textarea, [contenteditable]')) return;
      const at = driveLinks(event.clipboardData?.getData('text/plain') ?? '');
      if (!at) return;
      event.preventDefault();
      opener.current(at);
    };
    document.addEventListener('paste', paste);
    return () => document.removeEventListener('paste', paste);
  }, [picker]);
  const dialog = picker && (
    <>
      {choosing && (
        <FixturePicker
          picker={picker}
          initial={choosing}
          busy={adding.busy}
          close={() => setChoosing(undefined)}
          pick={async (files) => {
            if (await adding.run(files)) setChoosing(undefined);
          }}
        />
      )}
      {linking && <DriveLink close={() => setLinking(false)} open={open} />}
      {blocked && (
        <Modal title="Add from Google Drive" onClose={() => setBlocked(undefined)}>
          <div className="form-actions">
            <button onClick={() => setBlocked(undefined)}>Cancel</button>
            <button className="primary" onClick={() => open(blocked)}>
              Open Google Drive
            </button>
          </div>
        </Modal>
      )}
    </>
  );
  return { open: () => open(), askLink: () => setLinking(true), dialog };
}

/** Asks for a Google link, then opens the picker on what it names. */
function DriveLink({ close, open }: { close: () => void; open: (at: At) => void }) {
  const [link, setLink] = useState('');
  const [wrong, setWrong] = useState(false);
  return (
    <Modal title="Add a Drive link" onClose={close} initialFocus="input">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          const at = driveLinks(link);
          if (!at) return setWrong(true);
          close();
          open(at);
        }}
      >
        <label>
          Link
          <input
            value={link}
            placeholder="https://docs.google.com/…"
            onChange={(event) => {
              setLink(event.target.value);
              setWrong(false);
            }}
          />
        </label>
        <ErrorBox message={wrong ? "That isn't a link to a file or folder in Google Drive." : ''} />
        <div className="form-actions">
          <button type="button" onClick={close}>
            Cancel
          </button>
          <button className="primary" disabled={!link.trim()}>
            Add
          </button>
        </div>
      </form>
    </Modal>
  );
}

/** Fixture mode's stand-in for Google's picker: the files its Drive holds out of reach. */
function FixturePicker({
  picker,
  initial,
  busy,
  close,
  pick,
}: {
  picker: PickerSetup;
  initial: string[];
  busy: boolean;
  close: () => void;
  pick: (files: string[]) => void;
}) {
  const [chosen, setChosen] = useState<Set<string>>(
    () => new Set(initial.filter((id) => picker.madeInDrive.some((file) => file.id === id))),
  );
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
        <div className="documents-picked">
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
