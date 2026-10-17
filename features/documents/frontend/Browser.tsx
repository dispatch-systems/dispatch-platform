import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type DragEvent as ReactDragEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from 'react';
import {
  ChevronRight,
  Download,
  Ellipsis,
  ExternalLink,
  FileUp,
  FolderUp,
  HardDriveDownload,
  LayoutGrid,
  Link2,
  List,
  Plus,
  Upload,
} from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { useFeedback } from '../../../core/shell/frontend/runtime/feedback.js';
import { dspHash, hashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import {
  ConfirmDialog,
  DataState,
  Empty,
  Modal,
  Popover,
  Pagination,
  SearchInput,
  openContextMenu,
} from '../../../core/shell/frontend/ui/index.js';
import {
  createItem,
  downloadItem,
  foldersChanged,
  moveItem,
  prefetchFolder,
  renameItem,
  trashItem,
  useDocumentsFolder,
  type DocumentsConnection,
  type DocumentsOverview,
  type DocumentsItem,
  type FolderStep,
  type NewKind,
} from '../api/client.js';
import { Initials, Thumb, Tile, editedLabels, kindLabel, typeOf } from './items.js';
import { UploadNaming, UploadsPanel, dropped, picked, useUploads, type Picked } from './Uploads.js';
import { useFromDrive } from './FromDrive.js';
import { MoveTo, carryCount, type Place } from './Moving.js';

// The DSP's Documents, once Google is connected: a folder at a time, as a list or a grid,
// with what Documents makes, what anyone uploads, by picking files or dropping them on the
// page or on a folder, and what the team adds from Google Drive. A file opens in Google, in
// a tab of its own, or downloads. Right-clicking a file or folder opens its menu where the
// pointer is. Files and folders move by dragging them onto a folder or a step of the path, or
// with Move to…; a click with Shift, Ctrl or ⌘ selects several to move together. What's made,
// renamed, moved or trashed shows at once, and goes back if Google refuses it; a folder that
// hasn't come yet leaves the last one shown.

type Filter = 'all' | 'folders' | 'docs' | 'sheets' | 'slides' | 'uploads';
const FILTERS: [Filter, string][] = [
  ['all', 'All'],
  ['folders', 'Folders'],
  ['docs', 'Docs'],
  ['sheets', 'Sheets'],
  ['slides', 'Slides'],
  ['uploads', 'Uploads'],
];
const shown = (filter: Filter) => (item: DocumentsItem) => {
  switch (filter) {
    case 'all':
      return true;
    case 'folders':
      return item.kind === 'folder';
    case 'docs':
      return item.kind === 'doc';
    case 'sheets':
      return item.kind === 'sheet';
    case 'slides':
      return item.kind === 'slides';
    case 'uploads':
      return ['pdf', 'image', 'file'].includes(item.kind);
  }
};

const NEW: [NewKind, string, string][] = [
  ['folder', 'Folder', 'Untitled folder'],
  ['doc', 'Google Doc', 'Untitled document'],
  ['sheet', 'Google Sheet', 'Untitled spreadsheet'],
  ['slides', 'Google Slides', 'Untitled presentation'],
];

/** The folder the address names, followed as it changes. */
function useFolder() {
  return useSyncExternalStore(
    (changed) => {
      window.addEventListener('hashchange', changed);
      return () => window.removeEventListener('hashchange', changed);
    },
    () => hashQuery().get('folder') ?? undefined,
  );
}
/** `value`, once it stops changing for a moment. */
function useSettled(value: string, ms = 150) {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return settled;
}

const LAYOUT = 'documents.layout';
type Layout = 'list' | 'grid';

/** What the page is asked to do with an item. */
type Asked =
  | { to: 'make'; kind: NewKind }
  | { to: 'rename'; item: DocumentsItem }
  | { to: 'move'; items: DocumentsItem[] }
  | { to: 'trash'; item: DocumentsItem };

/**
 * A change the page shows before the folder it's in is read again with it: made, uploaded,
 * renamed, moved or trashed in `folder`. Something being made has a stand-in's ID until Google
 * makes it; something moved goes `to` a folder, or the top. Once Google confirmed it, it shows
 * until a reading of the folder does, or a while, and a move counts in the cards of the folders
 * it left and went into until they're read again (`counted`).
 */
type Pending = {
  key: string;
  folder?: string;
  change: 'made' | 'renamed' | 'moved' | 'trashed';
  item: DocumentsItem;
  to?: string;
  settled?: number;
  counted?: boolean;
};
const SETTLING_MS = 30_000;
/** Something Google moved, and the folder it was in: `null` for the top. */
type Moved = { item: DocumentsItem; from: string | null };
/** What the page's own drags carry, beside a link: the IDs of what's dragged. */
const CARRIED = 'application/x-dispatch-documents';
/** Whether `item` stands in for something Google is still making. */
const making = (item: DocumentsItem) => item.id.startsWith('pending:');
/** How a row or card shows: dimmed while Google makes it, or once its file is gone. */
const look = (item: DocumentsItem) =>
  making(item) ? ' documents-making' : item.unavailable ? ' documents-unavailable' : '';

/** Folders first, by name; then files, the most recently changed first: as the server sorts. */
function arranged(items: DocumentsItem[]) {
  const order = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  return [...items].sort(
    (a, b) =>
      Number(b.kind === 'folder') - Number(a.kind === 'folder') ||
      (a.kind === 'folder'
        ? order(a.name.toLowerCase(), b.name.toLowerCase())
        : order(b.modifiedAt, a.modifiedAt)),
  );
}
/** What a folder or a search shows, with the changes it hasn't been read with yet. */
function overlaid(
  items: DocumentsItem[],
  pending: Pending[],
  folder: string | undefined,
  searching: boolean,
) {
  const here = pending.filter((each) =>
    each.change === 'made' ? !searching && each.folder === folder : true,
  );
  if (!here.length) return items;
  const latest = (id: string) =>
    here.findLast((each) => each.change !== 'made' && each.item.id === id);
  // A folder's card counts what moved in or out of it until it's read again.
  const counted = (item: DocumentsItem) => {
    if (item.items === null) return item;
    const moved = here.filter((each) => each.change === 'moved' && !each.counted);
    const into = moved.filter((each) => each.to === item.id).length;
    const out = moved.filter((each) => each.folder === item.id).length;
    return into === out ? item : { ...item, items: Math.max(0, item.items + into - out) };
  };
  const shown = items.flatMap((item) => {
    const change = latest(item.id);
    if (!change) return [counted(item)];
    if (change.change === 'trashed') return [];
    // A search shows what moved where it went; a folder, only what's in it.
    if (change.change === 'moved' && !searching && change.to !== folder) return [];
    return [{ ...counted(item), name: change.item.name, location: change.item.location }];
  });
  if (searching) return shown;
  const arrived = here
    .filter(
      (each) =>
        (each.change === 'made' ||
          (each.change === 'moved' && each.to === folder && latest(each.item.id) === each)) &&
        !items.some((item) => item.id === each.item.id),
    )
    .map((each) => each.item);
  return arranged([...arrived, ...shown]);
}

/** The way to each folder the page has shown, so its path shows before the folder comes. */
const trails = new Map<string, FolderStep[]>();

export function Browser({
  view,
  overview,
  connection,
  overviewChanged,
}: {
  view: DspView;
  overview: DocumentsOverview;
  connection: DocumentsConnection;
  overviewChanged: () => void;
}) {
  const folder = useFolder();
  const [search, setSearch] = useState('');
  const query = useSettled(search);
  const [filter, setFilter] = useState<Filter>('all');
  const [layout, setLayout] = useState<Layout>(() =>
    localStorage.getItem(LAYOUT) === 'list' ? 'list' : 'grid',
  );
  const listing = useDocumentsFolder(folder, query);
  const [asked, ask] = useState<Asked>();
  const { perform, notify } = useFeedback();
  const [pending, setPending] = useState<Pending[]>([]);
  const [reveal, setReveal] = useState<string>();
  const changes = useRef(0);
  const show = (change: Omit<Pending, 'key'>) => {
    if (change.change === 'made' && change.folder === folder) setReveal(change.item.id);
    const key = `${change.change}:${changes.current++}`;
    setPending((all) => [...all, { ...change, key }]);
    return key;
  };
  const settle = (key: string, item?: DocumentsItem) =>
    setPending((all) =>
      all.map((each) =>
        each.key === key ? { ...each, item: item ?? each.item, settled: Date.now() } : each,
      ),
    );
  const drop = (key: string) => setPending((all) => all.filter((each) => each.key !== key));
  const uploads = useUploads(folder, (item, into) => {
    settle(show({ folder: into, change: 'made', item }), item);
    foldersChanged();
  });
  // Files picked or dropped on their own are named before they go up; a folder goes as it is.
  // They go into the folder open, or the folder they were dropped on.
  const [naming, setNaming] = useState<{ files: Picked[]; into?: string }>();
  const offer = (files: Picked[], into = folder) => {
    if (files.some((each) => !each.path.length)) setNaming({ files, into });
    else void uploads.start(files, into);
  };
  const fromDrive = useFromDrive(overview.picker, folder, () => {
    listing.refresh();
    overviewChanged();
  });
  const files = useRef<HTMLInputElement>(null);
  const folders = useRef<HTMLInputElement>(null);
  // Whether files from the computer are over the page, and the folder or step of the path under
  // the pointer as they or what the page carries are: '' for the top.
  const [filesOver, setFilesOver] = useState(false);
  const [over, setOver] = useState<string>();
  // What's selected, and the one Shift selects from.
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const anchor = useRef<string>(undefined);
  // What's being dragged, and the same once the browser has drawn it, to fade.
  const carried = useRef<DocumentsItem[]>(undefined);
  const [carrying, setCarrying] = useState<ReadonlySet<string>>();
  // A listing Google refused may mean the connection broke: the overview says.
  useEffect(() => {
    if (listing.error) overviewChanged();
  }, [listing.error, overviewChanged]);
  useEffect(() => setSearch(''), [folder]);
  useEffect(() => setSelected(new Set()), [folder, query, filter]);
  useEffect(() => {
    if (!selected.size) return;
    const clear = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setSelected(new Set());
    };
    document.addEventListener('keydown', clear);
    return () => document.removeEventListener('keydown', clear);
  }, [selected]);
  const choose = (next: Layout) => {
    setLayout(next);
    localStorage.setItem(LAYOUT, next);
  };
  const searching = Boolean(query.trim());
  const read = listing.data;
  // Until the folder comes, the last one shown stays, under the folder's own path.
  const at = read ?? listing.stale;
  const coming = !read && !listing.error && Boolean(at);
  const path = read?.path ?? (folder ? (trails.get(folder) ?? at?.path ?? []) : []);
  const top = !folder;
  const name = path.at(-1)?.name ?? read?.name ?? '';
  // What shows is a search's results while the last reading shown came from one.
  const [searchShown, setSearchShown] = useState(false);
  useEffect(() => {
    if (read) setSearchShown(searching);
  }, [read, searching]);
  const showsSearch = read ? searching : searchShown;
  useEffect(() => {
    if (!read || searching) return;
    read.path.forEach((_, index) =>
      trails.set(read.path[index]!.id, read.path.slice(0, index + 1)),
    );
    for (const item of read.items)
      if (item.kind === 'folder')
        trails.set(item.id, [...read.path, { id: item.id, name: item.name }]);
  }, [read, searching]);
  // A change Google confirmed goes once the folder is read with it, and a move stops counting
  // in its folders' cards once a reading after it does.
  useEffect(() => {
    if (!read) return;
    setPending((all) => {
      let changed = false;
      const left = all.flatMap((each) => {
        if (!each.settled) return [each];
        const listed = read.items.find((item) => item.id === each.item.id);
        const inside = !searching && each.folder === folder;
        const shown =
          Date.now() - each.settled > SETTLING_MS ||
          (each.change === 'renamed'
            ? listed?.name === each.item.name
            : each.change === 'made'
              ? inside && Boolean(listed)
              : each.change === 'moved'
                ? !searching && (each.to === folder ? Boolean(listed) : inside && !listed)
                : inside && !listed);
        changed ||= shown || (each.change === 'moved' && !each.counted);
        if (shown) return [];
        return [each.change === 'moved' ? { ...each, counted: true } : each];
      });
      return changed ? left : all;
    });
  }, [read, folder, searching]);
  const items = at ? overlaid(at.items, pending, folder, showsSearch).filter(shown(filter)) : [];
  const make = (kind: NewKind, title: string) => {
    // A tab opened by the click itself, so no popup blocker stops it; it goes to the file once
    // Google has made it.
    const tab = kind === 'folder' ? null : window.open('about:blank', '_blank');
    const into = folder;
    const id = `pending:${changes.current}`;
    const key = show({
      folder: into,
      change: 'made',
      item: {
        id,
        name: title,
        kind,
        size: null,
        modifiedAt: new Date().toISOString(),
        modifiedBy: null,
        addedBy: null,
        items: kind === 'folder' ? 0 : null,
        url: '',
        location: null,
        thumbnail: null,
        extension: null,
        shortcut: false,
        unavailable: false,
      },
    });
    void perform(
      async () => {
        try {
          const made = await createItem(into, kind, title);
          if (tab) tab.location.href = made.url;
          settle(key, made);
        } catch (error) {
          tab?.close();
          drop(key);
          throw error;
        }
      },
      `${kindLabel(kind)} created`,
    );
  };
  const rename = (item: DocumentsItem, title: string) => {
    const key = show({ folder, change: 'renamed', item: { ...item, name: title } });
    void perform(async () => {
      try {
        settle(key, await renameItem(item.id, title));
      } catch (error) {
        drop(key);
        throw error;
      }
    }, 'Renamed');
  };
  const trash = (item: DocumentsItem) => {
    const key = show({ folder, change: 'trashed', item });
    void perform(async () => {
      try {
        await trashItem(item.id);
        settle(key);
      } catch (error) {
        drop(key);
        throw error;
      }
    }, 'Moved to the trash');
  };
  /**
   * Moves `items` out of `from` into `to`, one at a time, as Google takes them; Undo takes each
   * back to the folder it was in.
   */
  const move = (moving: DocumentsItem[], to: Place, from = folder, undoing = false) => {
    const location = to.id ? to.name : null;
    const keys = moving.map((item) =>
      show({ folder: from, change: 'moved', item: { ...item, location }, to: to.id }),
    );
    setSelected(new Set());
    void perform(async () => {
      const moved: Moved[] = [];
      let failed: unknown;
      for (const [index, item] of moving.entries()) {
        try {
          const answer = await moveItem(item.id, to.id);
          settle(keys[index]!);
          moved.push({ item, from: answer.from });
        } catch (error) {
          drop(keys[index]!);
          failed ??= error;
        }
      }
      if (moved.length) {
        foldersChanged();
        if (undoing) notify('Move undone');
        else notify(`Moved to ${to.name}`, { label: 'Undo', run: () => undo(moved, to) });
      }
      if (failed instanceof Error) throw failed;
      if (failed) throw new Error('The move did not finish.');
    });
  };
  const undo = (moved: Moved[], to: Place) => {
    const back = new Map<string | null, DocumentsItem[]>();
    for (const { item, from } of moved) back.set(from, [...(back.get(from) ?? []), item]);
    for (const [from, group] of back) {
      // A search names the folder each was in; a folder, the one it is.
      const name = from
        ? (trails.get(from)?.at(-1)?.name ?? group[0]!.location ?? '')
        : 'Documents';
      move(group, { id: from ?? undefined, name }, to.id, true);
    }
  };
  /**
   * A click with Shift, Ctrl or ⌘ selects instead of opening: Ctrl or ⌘ adds or takes away
   * one, and Shift selects those from the last one picked.
   */
  const pick = (item: DocumentsItem, event: ReactMouseEvent) => {
    if (!(event.shiftKey || event.metaKey || event.ctrlKey) || making(item)) return;
    event.preventDefault();
    const ids = items.filter((each) => !making(each)).map((each) => each.id);
    const start = anchor.current ? ids.indexOf(anchor.current) : -1;
    if (event.shiftKey && start >= 0) {
      const end = ids.indexOf(item.id);
      setSelected(new Set(ids.slice(Math.min(start, end), Math.max(start, end) + 1)));
      return;
    }
    anchor.current = item.id;
    setSelected((all) => {
      const next = new Set(all);
      if (!next.delete(item.id)) next.add(item.id);
      return next;
    });
  };
  /** The selection, when `item` is in it, or else `item`. */
  const chosen = (item: DocumentsItem) =>
    selected.has(item.id) ? items.filter((each) => selected.has(each.id)) : [item];
  /** Starts dragging `item`, with the rest of the selection when it's in it. */
  const drag = (item: DocumentsItem) => (event: ReactDragEvent<HTMLElement>) => {
    const moving = chosen(item);
    carried.current = moving;
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(CARRIED, moving.map((each) => each.id).join(' '));
    if (moving.length > 1) carryCount(event.dataTransfer, moving.length);
    else {
      const box = event.currentTarget.getBoundingClientRect();
      event.dataTransfer.setDragImage(
        event.currentTarget,
        event.clientX - box.left,
        event.clientY - box.top,
      );
    }
    setTimeout(() => {
      if (carried.current === moving) setCarrying(new Set(moving.map((each) => each.id)));
    });
  };
  const dragEnd = () => {
    carried.current = undefined;
    setCarrying(undefined);
    setOver(undefined);
  };
  /**
   * What lets `place` take a drop: what the page carries moves into it, and files from the
   * computer upload into it. Nothing goes into itself.
   */
  const takes = (place: Place) => {
    const key = place.id ?? '';
    const fits = (event: ReactDragEvent) =>
      carried.current
        ? !carried.current.some((item) => item.id === place.id)
        : event.dataTransfer.types.includes('Files');
    return {
      onDragOver: (event: ReactDragEvent) => {
        if (!fits(event)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = carried.current ? 'move' : 'copy';
        setOver(key);
      },
      onDragLeave: (event: ReactDragEvent) => {
        if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
        setOver((now) => (now === key ? undefined : now));
      },
      onDrop: (event: ReactDragEvent) => {
        if (!fits(event)) return;
        event.preventDefault();
        event.stopPropagation();
        setFilesOver(false);
        const moving = carried.current;
        dragEnd();
        if (moving) move(moving, place);
        else void dropped(event.dataTransfer).then((files) => offer(files, place.id));
      },
    };
  };
  const moves: Moves = {
    selected,
    carrying,
    over,
    pick,
    drag,
    dragEnd,
    takes,
    moveTo: (item) => ask({ to: 'move', items: chosen(item) }),
  };
  // Where files from the computer go: the folder under the pointer, or the folder open.
  const overName =
    over === undefined
      ? undefined
      : over
        ? (items.find((item) => item.id === over)?.name ??
          path.find((step) => step.id === over)?.name)
        : 'Documents';
  return (
    <>
      <div className="page-heading documents-heading">
        <div>
          <h1>Documents</h1>
        </div>
        <div className="heading-actions">
          <a
            className="documents-link-button"
            href={read?.url ?? connection.folderUrl}
            target="_blank"
            rel="noreferrer"
          >
            <ExternalLink size={16} />
            Open in Drive
          </a>
          <Popover
            className="row-menu documents-upload"
            triggerClassName="documents-link-button"
            label="Upload"
            trigger={
              <>
                <Upload size={16} />
                Upload
              </>
            }
            anchored
          >
            <button onClick={() => files.current?.click()}>
              <span className="documents-menu-icon">
                <FileUp size={16} />
              </span>
              Files
            </button>
            <button onClick={() => folders.current?.click()}>
              <span className="documents-menu-icon">
                <FolderUp size={16} />
              </span>
              Folder
            </button>
            {overview.picker && (
              <>
                <span className="documents-menu-rule" role="separator" />
                <button onClick={fromDrive.askLink}>
                  <span className="documents-menu-icon">
                    <Link2 size={16} />
                  </span>
                  Drive link
                </button>
                <button onClick={fromDrive.open}>
                  <span className="documents-menu-icon">
                    <HardDriveDownload size={16} />
                  </span>
                  Google Drive
                </button>
              </>
            )}
          </Popover>
          <Popover
            className="row-menu documents-new"
            label="New"
            trigger={
              <>
                <Plus size={16} />
                New
              </>
            }
            anchored
          >
            {NEW.map(([kind, label]) => (
              <button key={kind} onClick={() => ask({ to: 'make', kind })}>
                <Tile kind={kind} />
                {label}
              </button>
            ))}
          </Popover>
          <input
            ref={files}
            type="file"
            multiple
            hidden
            aria-label="Files to upload"
            onChange={(event) => {
              offer(picked(event.target.files));
              event.target.value = '';
            }}
          />
          <input
            ref={(input) => {
              folders.current = input;
              if (input) input.webkitdirectory = true;
            }}
            type="file"
            hidden
            aria-label="Folder to upload"
            onChange={(event) => {
              offer(picked(event.target.files));
              event.target.value = '';
            }}
          />
        </div>
      </div>
      <div className="documents-toolbar">
        {path.length > 0 ? (
          <Path view={view} path={path} moves={moves} />
        ) : (
          <div className="documents-pills" role="group" aria-label="Show">
            {FILTERS.map(([value, label]) => (
              <button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>
                {label}
              </button>
            ))}
          </div>
        )}
        <div>
          <SearchInput
            type="search"
            label={top ? 'Search Documents' : 'Search this folder'}
            placeholder={top ? 'Search Documents' : 'Search this folder'}
            value={search}
            onChange={setSearch}
          />
          <div className="documents-layout" role="group" aria-label="View">
            <button
              aria-pressed={layout === 'list'}
              aria-label="List"
              onClick={() => choose('list')}
            >
              <List size={16} />
            </button>
            <button
              aria-pressed={layout === 'grid'}
              aria-label="Grid"
              onClick={() => choose('grid')}
            >
              <LayoutGrid size={16} />
            </button>
          </div>
        </div>
      </div>
      <div
        className={`documents-drop${filesOver ? ' dragging' : ''}`}
        onDragOver={(event) => {
          if (carried.current || !event.dataTransfer.types.includes('Files')) return;
          event.preventDefault();
          setFilesOver(true);
        }}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null))
            setFilesOver(false);
        }}
        onDrop={(event) => {
          if (carried.current || !event.dataTransfer.types.includes('Files')) return;
          event.preventDefault();
          setFilesOver(false);
          void dropped(event.dataTransfer).then((files) => offer(files));
        }}
        onClick={(event) => {
          // A click beside everything lets go of the selection.
          if (selected.size && !(event.target as Element).closest('[data-item]'))
            setSelected(new Set());
        }}
      >
        {filesOver && (
          <div className="documents-drop-hint" aria-hidden>
            <span>
              <Upload size={18} />
              Drop to upload to {overName ?? (path.length ? name : 'Documents')}
            </span>
          </div>
        )}
        {coming && <div className="documents-progress" role="status" aria-label="Loading" />}
        <DataState data={at} error={listing.error} retry={listing.refresh}>
          {(at) => {
            if (!items.length)
              return showsSearch ? (
                <Empty title="Nothing found">
                  No file or folder {top ? 'in Documents' : `in ${name}`} has that in its name.
                </Empty>
              ) : at.items.length ? (
                <Empty title={`No ${FILTERS.find(([value]) => value === filter)![1]} here`} />
              ) : (
                <Empty title={top ? 'Nothing here yet' : `${name} is empty`}>
                  Use New to add a folder, a Google Doc, a Sheet or Slides, or drop files here to
                  upload them.
                </Empty>
              );
            return (
              <div className={coming ? 'documents-coming' : undefined}>
                <Listing
                  key={`${folder}:${query}:${filter}`}
                  view={view}
                  items={items}
                  searching={showsSearch}
                  ask={ask}
                  moves={moves}
                  layout={layout}
                  reveal={reveal}
                  revealed={() => setReveal(undefined)}
                />
              </div>
            );
          }}
        </DataState>
      </div>
      <UploadsPanel uploads={uploads.uploads} clear={uploads.clear} />
      {naming && (
        <UploadNaming
          files={naming.files}
          upload={(files) => void uploads.start(files, naming.into)}
          done={() => setNaming(undefined)}
        />
      )}
      {fromDrive.dialog}
      {asked?.to === 'make' && (
        <Naming
          title={`New ${NEW.find(([kind]) => kind === asked.kind)![1]}`}
          initial={NEW.find(([kind]) => kind === asked.kind)![2]}
          confirm="Create"
          done={() => ask(undefined)}
          save={(name) => make(asked.kind, name)}
        />
      )}
      {asked?.to === 'rename' && (
        <Naming
          title={`Rename ${asked.item.kind === 'folder' ? 'folder' : 'file'}`}
          initial={asked.item.name.slice(
            0,
            asked.item.name.length - (asked.item.extension ?? '').length,
          )}
          extension={asked.item.extension ?? ''}
          confirm="Rename"
          done={() => ask(undefined)}
          save={(name) => rename(asked.item, name)}
        />
      )}
      {asked?.to === 'move' && (
        <MoveTo
          items={asked.items}
          path={path}
          from={showsSearch ? undefined : (folder ?? null)}
          move={(to) => move(asked.items, to)}
          done={() => ask(undefined)}
        />
      )}
      {asked?.to === 'trash' && (
        <Trash item={asked.item} done={() => ask(undefined)} trash={() => trash(asked.item)} />
      )}
    </>
  );
}

/** The way from the top of Documents to the folder open now: each step above it takes drops. */
function Path({ view, path, moves }: { view: DspView; path: FolderStep[]; moves: Moves }) {
  const dropOn = (place: Place) => ({
    className: moves.over === (place.id ?? '') ? 'documents-over' : undefined,
    ...moves.takes(place),
  });
  return (
    <nav className="documents-path" aria-label="Folder">
      <a
        href={dspHash(view.dsp.id, 'documents')}
        onPointerEnter={() => prefetchFolder()}
        onFocus={() => prefetchFolder()}
        {...dropOn({ name: 'Documents' })}
      >
        Documents
      </a>
      {path.map((step, index) => (
        <span key={step.id}>
          <ChevronRight size={14} aria-hidden="true" />
          {index === path.length - 1 ? (
            <strong aria-current="page">{step.name}</strong>
          ) : (
            <a
              href={dspHash(view.dsp.id, 'documents', { folder: step.id })}
              onPointerEnter={() => prefetchFolder(step.id)}
              onFocus={() => prefetchFolder(step.id)}
              {...dropOn({ id: step.id, name: step.name })}
            >
              {step.name}
            </a>
          )}
        </span>
      ))}
    </nav>
  );
}

type ViewProps = {
  view: DspView;
  items: DocumentsItem[];
  searching: boolean;
  ask: (asked: Asked) => void;
  moves: Moves;
};

/** Selecting, dragging and dropping, as the page keeps them. */
type Moves = {
  selected: ReadonlySet<string>;
  /** What's being dragged, once the browser has drawn it. */
  carrying?: ReadonlySet<string>;
  /** The folder under what's dragged: '' for the top. */
  over?: string;
  pick: (item: DocumentsItem, event: ReactMouseEvent) => void;
  drag: (item: DocumentsItem) => (event: ReactDragEvent<HTMLElement>) => void;
  dragEnd: () => void;
  takes: (place: Place) => {
    onDragOver: (event: ReactDragEvent) => void;
    onDragLeave: (event: ReactDragEvent) => void;
    onDrop: (event: ReactDragEvent) => void;
  };
  /** Asks where to move `item`, with the rest of the selection when it's in it. */
  moveTo: (item: DocumentsItem) => void;
};
/** How a row or card shows: as `look` has it, and selected, dragged or dropped on. */
const marked = (item: DocumentsItem, moves: Moves) =>
  look(item) +
  (moves.selected.has(item.id) ? ' documents-selected' : '') +
  (moves.carrying?.has(item.id) ? ' documents-carried' : '') +
  (moves.over === item.id ? ' documents-over' : '');
/**
 * What a row or card takes to be selected and dragged, and a folder's to take drops. One Google
 * is still making is neither.
 */
const handles = (item: DocumentsItem, moves: Moves) => ({
  'data-item': '',
  ...(!making(item) && {
    draggable: true,
    onClick: (event: ReactMouseEvent) => moves.pick(item, event),
    onDragStart: moves.drag(item),
    onDragEnd: moves.dragEnd,
    ...(item.kind === 'folder' && moves.takes({ id: item.id, name: item.name })),
  }),
});

/** Both layouts keep only one page mounted, including row menus and thumbnails. */
function Listing({
  layout,
  reveal,
  revealed,
  ...props
}: ViewProps & {
  layout: Layout;
  reveal?: string;
  revealed: () => void;
}) {
  const [selected, setPage] = useState(0);
  const pageSize = 100;
  const page = Math.min(selected, Math.max(0, Math.ceil(props.items.length / pageSize) - 1));
  useEffect(() => {
    if (selected !== page) setPage(page);
  }, [selected, page]);
  useEffect(() => {
    if (!reveal) return;
    const index = props.items.findIndex((item) => item.id === reveal);
    if (index >= 0) setPage(Math.floor(index / pageSize));
    revealed();
  }, [reveal, revealed, props.items]);
  const visible = { ...props, items: props.items.slice(page * pageSize, (page + 1) * pageSize) };
  return (
    <>
      {layout === 'list' ? <Table {...visible} /> : <Grid {...visible} />}
      <Pagination page={page} pageSize={pageSize} total={props.items.length} onChange={setPage} />
    </>
  );
}

/**
 * Where an item opens: a folder in Documents, read as the pointer rests on it, and a file in
 * Google, in a tab of its own. One Google is still making opens nowhere yet.
 */
function Opens({
  view,
  item,
  className,
  children,
}: {
  view: DspView;
  item: DocumentsItem;
  className?: string;
  children: ReactNode;
}) {
  if (making(item) || item.unavailable) return <span className={className}>{children}</span>;
  return item.kind === 'folder' ? (
    <a
      className={className}
      href={dspHash(view.dsp.id, 'documents', { folder: item.id })}
      onPointerEnter={() => prefetchFolder(item.id)}
      onFocus={() => prefetchFolder(item.id)}
    >
      {children}
    </a>
  ) : (
    <a className={className} href={item.url} target="_blank" rel="noreferrer">
      {children}
    </a>
  );
}

function Actions({
  item,
  ask,
  moves,
}: {
  item: DocumentsItem;
  ask: (asked: Asked) => void;
  moves: Moves;
}) {
  const saving = useAction(() => downloadItem(item.id));
  return (
    <Popover
      className="row-menu"
      label={`Actions for ${item.name}`}
      trigger={<Ellipsis size={18} />}
      anchored
      lazy
    >
      {item.kind !== 'folder' && !item.unavailable && (
        <a href={item.url} target="_blank" rel="noreferrer">
          <ExternalLink size={14} />
          Open in Google
        </a>
      )}
      {item.kind !== 'folder' && !item.unavailable && (
        <button disabled={saving.busy} onClick={() => void saving.run()}>
          <Download size={14} />
          {downloadLabel(item)}
        </button>
      )}
      {!item.shortcut && <button onClick={() => ask({ to: 'rename', item })}>Rename</button>}
      <button onClick={() => moves.moveTo(item)}>Move to…</button>
      <button className="danger" onClick={() => ask({ to: 'trash', item })}>
        Move to trash
      </button>
    </Popover>
  );
}

/** What a file downloads as: Google's own as the Office files they export as. */
const downloadLabel = (item: DocumentsItem) =>
  ({ doc: 'Download as Word', sheet: 'Download as Excel', slides: 'Download as PowerPoint' })[
    item.kind as 'doc' | 'sheet' | 'slides'
  ] ?? 'Download';

function Table({ view, items, searching, ask, moves }: ViewProps) {
  const labels = editedLabels(view.dsp.timezone);
  return (
    <div className="table-wrap">
      <table className="documents-table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Type</th>
            <th>Last edited</th>
            <th>Added by</th>
            <th>
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {items.map((item) => (
            <tr
              key={item.id}
              className={marked(item, moves).trim() || undefined}
              onContextMenu={openContextMenu}
              {...handles(item, moves)}
            >
              <td>
                <Opens view={view} item={item} className="documents-name">
                  <Tile kind={item.kind} />
                  <span>
                    <strong>{item.name}</strong>
                    {searching && <small>in {item.location ?? 'Documents'}</small>}
                  </span>
                </Opens>
              </td>
              <td className="muted">{typeOf(item)}</td>
              <td>
                {item.unavailable ? (
                  <span className="muted">Unavailable</span>
                ) : (
                  <span className="documents-edited">
                    {labels.edited(item.modifiedAt)}
                    {item.modifiedBy && <small>{item.modifiedBy}</small>}
                  </span>
                )}
              </td>
              <td>
                {item.addedBy ? (
                  <span className="documents-person">
                    <Initials name={item.addedBy} />
                    {item.addedBy}
                  </span>
                ) : (
                  <span className="muted">—</span>
                )}
              </td>
              <td className="cell-end">
                {!making(item) && <Actions item={item} ask={ask} moves={moves} />}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Grid({ view, items, searching, ask, moves }: ViewProps) {
  const labels = editedLabels(view.dsp.timezone);
  const folders = items.filter((item) => item.kind === 'folder');
  const files = items.filter((item) => item.kind !== 'folder');
  return (
    <div className="documents-cards">
      {folders.length > 0 && (
        <section aria-label="Folders">
          <h2>Folders</h2>
          <div className="documents-card-grid">
            {folders.map((item) => (
              <div
                className={`documents-folder-card${marked(item, moves)}`}
                key={item.id}
                onContextMenu={openContextMenu}
                {...handles(item, moves)}
              >
                <Opens view={view} item={item} className="documents-name">
                  <Tile kind="folder" size={18} />
                  <span>
                    <strong>{item.name}</strong>
                    <small>{searching ? `in ${item.location ?? 'Documents'}` : typeOf(item)}</small>
                  </span>
                </Opens>
                {!making(item) && <Actions item={item} ask={ask} moves={moves} />}
              </div>
            ))}
          </div>
        </section>
      )}
      {files.length > 0 && (
        <section aria-label="Files">
          <h2>Files</h2>
          <div className="documents-card-grid">
            {files.map((item) => (
              <div
                className={`documents-file-card${marked(item, moves)}`}
                key={item.id}
                onContextMenu={openContextMenu}
                {...handles(item, moves)}
              >
                <Opens view={view} item={item} className="documents-file-open">
                  <Thumb kind={item.kind} picture={item.thumbnail} />
                  <span className="documents-name">
                    <Tile kind={item.kind} />
                    <span>
                      <strong>{item.name}</strong>
                      <small>
                        {searching
                          ? `in ${item.location ?? 'Documents'}`
                          : item.unavailable
                            ? 'Unavailable'
                            : `Edited ${labels.inline(item.modifiedAt)}${item.modifiedBy ? ` by ${item.modifiedBy}` : ''}`}
                      </small>
                    </span>
                  </span>
                </Opens>
                {!making(item) && <Actions item={item} ask={ask} moves={moves} />}
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

/**
 * Asks for a name, to make something or rename it: a file's `extension` stays, unshown. It
 * closes as the name is given; the page shows the change while Google makes it.
 */
function Naming({
  title,
  initial,
  extension = '',
  confirm,
  save,
  done,
}: {
  title: string;
  initial: string;
  extension?: string;
  confirm: string;
  save: (name: string) => void;
  done: () => void;
}) {
  const [name, setName] = useState(initial);
  return (
    <Modal title={title} onClose={done} initialFocus="input">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          save(name.trim() + extension);
          done();
        }}
      >
        <label>
          Name
          <input
            value={name}
            maxLength={200 - extension.length}
            required
            onChange={(event) => setName(event.target.value)}
            onFocus={(event) => event.target.select()}
          />
        </label>
        <div className="form-actions">
          <button type="button" onClick={done}>
            Cancel
          </button>
          <button className="primary" disabled={!name.trim()}>
            {confirm}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function Trash({
  item,
  done,
  trash,
}: {
  item: DocumentsItem;
  done: () => void;
  trash: () => void;
}) {
  return (
    <ConfirmDialog
      title={`Move ${item.name} to the trash?`}
      confirm="Move to trash"
      tone="danger"
      onConfirm={() => {
        trash();
        done();
      }}
      onCancel={done}
    >
      {item.kind === 'folder'
        ? 'It and everything in it go to the trash of the Google account that holds Documents, for 30 days. Its owner can restore them from Google Drive.'
        : 'It goes to the trash of the Google account that holds Documents, for 30 days. Its owner can restore it from Google Drive.'}
    </ConfirmDialog>
  );
}
