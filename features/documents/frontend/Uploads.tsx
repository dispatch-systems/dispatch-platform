import { useRef, useState } from 'react';
import { CircleAlert, CircleCheck, LoaderCircle, X } from 'lucide-react';
import { messageOf } from '../../../core/shell/frontend/lib/errors.js';
import { Modal } from '../../../core/shell/frontend/ui/index.js';
import { UPLOAD_LIMIT, createItem, uploadFile } from '../api/client.js';
import { size } from './items.js';

// Uploading files and folders into the folder open, from a picker or dropped on the page:
// the files picked on their own are named first, a folder's folders are made, then the files
// go up two at a time, each showing how far it got.

/**
 * A file to upload, with the folders it sits in inside what was picked or dropped, and the
 * name it was given, when not its own.
 */
export type Picked = { path: string[]; file: File; name?: string };
type Upload = {
  key: number;
  name: string;
  bytes: number;
  sent: number;
  state: 'waiting' | 'uploading' | 'done' | 'failed';
  error?: string;
};
/** Files at once: the server takes a few uploads at a time, for every DSP. */
const AT_ONCE = 2;

/** The files a picker gave, each in the folders a folder picker names. */
export const picked = (files: FileList | null): Picked[] =>
  [...(files ?? [])].map((file) => ({
    path: file.webkitRelativePath.split('/').slice(0, -1),
    file,
  }));

/** The files a drop holds, read from inside any folder dropped with them. */
export async function dropped(transfer: DataTransfer): Promise<Picked[]> {
  // The entries are only there while the drop is handled, so they are read first.
  const entries = [...transfer.items]
    .map((item) => item.webkitGetAsEntry?.())
    .filter((entry): entry is FileSystemEntry => Boolean(entry));
  if (!entries.length) return [...transfer.files].map((file) => ({ path: [], file }));
  const found: Picked[] = [];
  const walk = async (entry: FileSystemEntry, path: string[]): Promise<void> => {
    if (entry.isFile) {
      const file = await new Promise<File>((resolve, reject) =>
        (entry as FileSystemFileEntry).file(resolve, reject),
      );
      found.push({ path, file });
    } else if (entry.isDirectory) {
      const reader = (entry as FileSystemDirectoryEntry).createReader();
      // A folder's entries come a batch at a time, until an empty one.
      for (;;) {
        const batch = await new Promise<FileSystemEntry[]>((resolve, reject) =>
          reader.readEntries(resolve, reject),
        );
        if (!batch.length) break;
        for (const child of batch) await walk(child, [...path, entry.name]);
      }
    }
  };
  for (const entry of entries) await walk(entry, []);
  return found;
}

/** Uploads into `folder`, or the top, calling `uploaded` as each file lands. */
export function useUploads(folder: string | undefined, uploaded: () => void) {
  const [uploads, setUploads] = useState<Upload[]>([]);
  const next = useRef(0);
  const change = (key: number, update: Partial<Upload>) =>
    setUploads((all) => all.map((each) => (each.key === key ? { ...each, ...update } : each)));
  async function start(files: Picked[]) {
    if (!files.length) return;
    const queued = files.map(({ path, file, name }) => {
      const upload: Upload = {
        key: next.current++,
        name: name ?? file.name,
        bytes: file.size,
        sent: 0,
        state: file.size > UPLOAD_LIMIT ? 'failed' : 'waiting',
        error: file.size > UPLOAD_LIMIT ? 'Files can be up to 100 MB.' : undefined,
      };
      return { path, file, upload };
    });
    setUploads((all) => [
      ...all.filter((each) => each.state !== 'done'),
      ...queued.map((q) => q.upload),
    ]);
    // The folders a folder held, each made once, inside the one before it.
    const folders = new Map<string, Promise<string | undefined>>();
    const into = (path: string[]): Promise<string | undefined> => {
      if (!path.length) return Promise.resolve(folder);
      const key = path.join('/');
      if (!folders.has(key))
        folders.set(
          key,
          into(path.slice(0, -1)).then(
            async (parent) => (await createItem(parent, 'folder', path.at(-1)!)).id,
          ),
        );
      return folders.get(key)!;
    };
    const waiting = queued.filter((q) => q.upload.state === 'waiting');
    const work = async () => {
      for (let each = waiting.shift(); each; each = waiting.shift()) {
        const { key } = each.upload;
        change(key, { state: 'uploading' });
        try {
          const parent = await into(each.path);
          await uploadFile(each.file, each.upload.name, parent, (sent) => change(key, { sent }));
          change(key, { state: 'done', sent: each.file.size });
          uploaded();
        } catch (error) {
          change(key, { state: 'failed', error: messageOf(error) });
        }
      }
    };
    await Promise.all(Array.from({ length: AT_ONCE }, work));
  }
  const clear = () => setUploads((all) => all.filter((each) => each.state === 'uploading'));
  return { uploads, start, clear };
}

/** The extension a file's name keeps, such as `.pdf`, as the server reads one: none if not. */
export function extension(name: string) {
  const dot = name.lastIndexOf('.');
  return dot > 0 && /^[A-Za-z0-9]{1,8}$/.test(name.slice(dot + 1)) ? name.slice(dot) : '';
}

/**
 * Asks what to name the files picked or dropped on their own before any goes up. Each keeps
 * its extension, which isn't shown, so it can't be changed by accident. A file inside a folder
 * that came with them keeps its name, and one too large is left out.
 */
export function UploadNaming({
  files,
  upload,
  done,
}: {
  files: Picked[];
  upload: (files: Picked[]) => void;
  done: () => void;
}) {
  const loose = files.filter((each) => !each.path.length);
  const inFolders = files.filter((each) => each.path.length);
  const [names, setNames] = useState(() =>
    loose.map(({ file }) => file.name.slice(0, file.name.length - extension(file.name).length)),
  );
  const fits = (at: number) => loose[at]!.file.size <= UPLOAD_LIMIT;
  const named = (at: number) => names[at]!.trim() + extension(loose[at]!.file.name);
  const ready =
    (inFolders.length > 0 || loose.some((_, at) => fits(at))) &&
    loose.every((_, at) => !fits(at) || (names[at]!.trim() && named(at).length <= 200));
  const one = loose.length === 1;
  return (
    <Modal
      title={one ? 'Upload file' : `Upload ${loose.length} files`}
      onClose={done}
      initialFocus="input"
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (!ready) return;
          upload([
            ...loose.flatMap((each, at) => (fits(at) ? [{ ...each, name: named(at) }] : [])),
            ...inFolders,
          ]);
          done();
        }}
      >
        <ul className="documents-naming">
          {loose.map(({ file }, at) => (
            <li key={at}>
              {fits(at) ? (
                <label>
                  {one && 'Name'}
                  <input
                    value={names[at]}
                    maxLength={200 - extension(file.name).length}
                    required
                    aria-label={one ? undefined : `Name of ${file.name}`}
                    onChange={(event) =>
                      setNames((all) =>
                        all.map((name, i) => (i === at ? event.target.value : name)),
                      )
                    }
                    onFocus={(event) => event.target.select()}
                  />
                </label>
              ) : (
                <span>{file.name}</span>
              )}
              <small className={fits(at) ? 'muted' : 'documents-too-large'}>
                {fits(at) ? size(file.size) : 'Files can be up to 100 MB.'}
              </small>
            </li>
          ))}
        </ul>
        {inFolders.length > 0 && (
          <p className="muted">
            {inFolders.length === 1
              ? '1 file in a folder goes up with its own name.'
              : `${inFolders.length} files in folders go up with their own names.`}
          </p>
        )}
        <div className="form-actions">
          <button type="button" onClick={done}>
            Cancel
          </button>
          <button className="primary" disabled={!ready}>
            Upload
          </button>
        </div>
      </form>
    </Modal>
  );
}

/** How the uploads are going, in the corner of the page. */
export function UploadsPanel({ uploads, clear }: { uploads: Upload[]; clear: () => void }) {
  if (!uploads.length) return null;
  const going = uploads.filter((each) => each.state === 'waiting' || each.state === 'uploading');
  const failed = uploads.filter((each) => each.state === 'failed').length;
  const done = uploads.filter((each) => each.state === 'done').length;
  const title = going.length
    ? `Uploading ${going.length} ${going.length === 1 ? 'file' : 'files'}`
    : [done && `${done} uploaded`, failed && `${failed} couldn't upload`]
        .filter(Boolean)
        .join(', ');
  return (
    <section className="documents-uploads" aria-label="Uploads" aria-live="polite">
      <header>
        <strong>{title}</strong>
        {!going.length && (
          <button aria-label="Close uploads" onClick={clear}>
            <X size={16} />
          </button>
        )}
      </header>
      <ul>
        {uploads.map((each) => (
          <li key={each.key} className={each.state}>
            {each.state === 'done' ? (
              <CircleCheck size={16} />
            ) : each.state === 'failed' ? (
              <CircleAlert size={16} />
            ) : (
              <LoaderCircle size={16} className="spin" />
            )}
            <div>
              <span>{each.name}</span>
              {each.state === 'failed' ? (
                <small>{each.error}</small>
              ) : each.state === 'done' ? (
                <small>{size(each.bytes)}</small>
              ) : (
                <span className="documents-meter" role="presentation">
                  <span style={{ width: `${each.bytes ? (each.sent / each.bytes) * 100 : 0}%` }} />
                </span>
              )}
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
