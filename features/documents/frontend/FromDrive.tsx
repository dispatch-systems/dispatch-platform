import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../../core/shell/frontend/runtime/api.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { useFeedback } from '../../../core/shell/frontend/runtime/feedback.js';
import { ErrorBox, Modal } from '../../../core/shell/frontend/ui/index.js';
import { addFiles, type PickerSetup } from '../api/client.js';
import { driveLinks } from './driveLinks.js';
import { Tile } from './items.js';

// Adding files and folders from Google Drive, for every member. A Google link, given in a dialog
// or pasted on the page, adds what it names at once when the account that holds Documents
// reaches it. Else, and from Google Drive in the Upload menu, Google's picker opens on a page of
// its own in this tab, signed in to Google as the member, and brings what they pick where that
// account reaches it, then comes back here to add it. Fixture mode has no picker of Google's: it
// lists its Drive's files here instead.

const PICKER = '/api/documents/google/picker';
/** What the picker's page is to pick, and where it goes back to, for this tab only. */
const ASKED = 'dispatch-documents-picker';
/** What it picked, left for the page it went back to. */
const PICKED = 'dispatch-documents-picked';

/** What a link names: files, or a folder. */
type At = { files: string[]; folder?: string };

/** Names as a sentence lists them: "A", "A or B", "A, B or C". */
const listed = (names: string[]) =>
  names.length > 1 ? `${names.slice(0, -1).join(', ')} or ${names.at(-1)}` : names[0];

/** What the picker's page picked, once, if it went back to this page. */
function takePicked() {
  const left = sessionStorage.getItem(PICKED);
  if (!left) return undefined;
  sessionStorage.removeItem(PICKED);
  try {
    const { files, at } = JSON.parse(left) as { files?: string[]; at?: string };
    const page = (href: string) => href.split('?')[0];
    return files?.length && at && page(at) === page(location.href) ? files : undefined;
  } catch {
    return undefined;
  }
}

/** Adds files from Drive into `folder`, or the top, calling `added` once they're in. */
export function useFromDrive(
  picker: PickerSetup | null,
  folder: string | undefined,
  added: () => void,
) {
  // Fixture mode's list, with the files a pasted link named chosen.
  const [choosing, setChoosing] = useState<string[]>();
  const [linking, setLinking] = useState(false);
  const { notify } = useFeedback();
  async function land(files: string[]) {
    const { unshared } = await addFiles(files, folder);
    added();
    notify(
      unshared.length
        ? `Added to Documents. You can't share ${listed(unshared)}, so teammates ask its owner to open it.`
        : 'Added to Documents',
    );
  }
  const adding = useAction(land);
  // What a link names comes in at once when the account that holds Documents reaches it; else
  // the member picks it as themselves.
  const reaching = useAction(async (files: string[]) => {
    try {
      await land(files);
    } catch (error) {
      if (!(error instanceof ApiError) || error.code !== 'documents_picked_elsewhere') throw error;
      pick(files);
    }
  });
  function pick(files: string[]) {
    if (!picker) return;
    if (!picker.google) return setChoosing(files);
    sessionStorage.setItem(
      ASKED,
      JSON.stringify({
        ...picker.google,
        me: picker.me,
        account: picker.account,
        into: folder ?? picker.folder,
        back: location.href,
        ...(files.length ? { files: files.join(',') } : {}),
      }),
    );
    location.assign(PICKER);
  }
  function open(at?: At) {
    const files = at?.folder ? [at.folder] : (at?.files ?? []);
    if (files.length) void reaching.run(files);
    else pick([]);
  }
  // What the picker picked, once it's back here.
  const ready = Boolean(picker);
  useEffect(() => {
    if (!ready) return;
    const files = takePicked();
    if (files) void adding.run(files);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `adding` is made anew each render; the picker's return is what runs it, once
  }, [ready]);
  // A Google link pasted anywhere but a field adds what it names.
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

/** Fixture mode's stand-in for Google's picker: the files its Drive holds outside Documents. */
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
    () => new Set(initial.filter((id) => picker.elsewhere.some((file) => file.id === id))),
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
        Files in {picker.account}'s Google Drive outside Documents, as Google's picker would show
        them.
      </p>
      {picker.elsewhere.length ? (
        <div className="documents-picked">
          {picker.elsewhere.map((file) => (
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
