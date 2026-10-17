import { useEffect, useRef, useState } from 'react';
import {
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
import { DataState, Empty, Popover, SearchInput } from '../../../core/shell/frontend/ui/index.js';
import {
  createItem,
  foldersChanged,
  moveItem,
  renameItem,
  trashItem,
  useDocumentsFolder,
  type DocumentsConnection,
  type DocumentsOverview,
  type DocumentsItem,
  type NewKind,
} from '../api/client.js';
import { Tile, kindLabel } from './items.js';
import { UploadNaming, UploadsPanel, dropped, picked, useUploads, type Picked } from './Uploads.js';
import { useFromDrive } from './FromDrive.js';
import { MoveTo, type Place } from './Moving.js';
import {
  FILTERS,
  LAYOUT,
  NEW,
  overlaid,
  shown,
  trails,
  useFolder,
  useSettled,
  type Asked,
  type Filter,
  type Layout,
  type Moved,
} from './browser/browsing.js';
import { Naming, Trash } from './browser/dialogs.js';
import { useDragging } from './browser/useDragging.js';
import { usePending } from './browser/usePending.js';
import { useSelection } from './browser/useSelection.js';
import { Listing, Path, type Moves } from './browser/views.js';

// The DSP's Documents, once Google is connected: a folder at a time, as a list or a grid,
// with what Documents makes, what anyone uploads, by picking files or dropping them on the
// page or on a folder, and what the team adds from Google Drive. A file opens in Google, in
// a tab of its own, or downloads. Right-clicking a file or folder opens its menu where the
// pointer is. Files and folders move by dragging them onto a folder or a step of the path, or
// with Move to…; a click with Shift, Ctrl or ⌘ selects several to move together. What's made,
// renamed, moved or trashed shows at once, and goes back if Google refuses it; a folder that
// hasn't come yet leaves the last one shown. What the page draws is in `browser/views.tsx`;
// the changes it shows ahead of Google, the selection and dragging are each a hook there.

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
  const searching = Boolean(query.trim());
  const read = listing.data;
  const changes = usePending(folder, read, searching);
  const uploads = useUploads(folder, (item, into) => {
    changes.settle(changes.show({ folder: into, change: 'made', item }), item);
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
  // A listing Google refused may mean the connection broke: the overview says.
  useEffect(() => {
    if (listing.error) overviewChanged();
  }, [listing.error, overviewChanged]);
  useEffect(() => setSearch(''), [folder]);
  const choose = (next: Layout) => {
    setLayout(next);
    localStorage.setItem(LAYOUT, next);
  };
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
  const items = at
    ? overlaid(at.items, changes.pending, folder, showsSearch).filter(shown(filter))
    : [];
  const selection = useSelection(items, [folder, query, filter]);
  const make = (kind: NewKind, title: string) => {
    // A tab opened by the click itself, so no popup blocker stops it; it goes to the file once
    // Google has made it.
    const tab = kind === 'folder' ? null : window.open('about:blank', '_blank');
    const into = folder;
    const key = changes.show({
      folder: into,
      change: 'made',
      item: {
        id: changes.standIn(),
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
          changes.settle(key, made);
        } catch (error) {
          tab?.close();
          changes.drop(key);
          throw error;
        }
      },
      `${kindLabel(kind)} created`,
    );
  };
  const rename = (item: DocumentsItem, title: string) => {
    const key = changes.show({ folder, change: 'renamed', item: { ...item, name: title } });
    void perform(async () => {
      try {
        changes.settle(key, await renameItem(item.id, title));
      } catch (error) {
        changes.drop(key);
        throw error;
      }
    }, 'Renamed');
  };
  const trash = (item: DocumentsItem) => {
    const key = changes.show({ folder, change: 'trashed', item });
    void perform(async () => {
      try {
        await trashItem(item.id);
        changes.settle(key);
      } catch (error) {
        changes.drop(key);
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
      changes.show({ folder: from, change: 'moved', item: { ...item, location }, to: to.id }),
    );
    selection.clear();
    void perform(async () => {
      const moved: Moved[] = [];
      let failed: unknown;
      for (const [index, item] of moving.entries()) {
        try {
          const answer = await moveItem(item.id, to.id);
          changes.settle(keys[index]!);
          moved.push({ item, from: answer.from });
        } catch (error) {
          changes.drop(keys[index]!);
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
  const dragging = useDragging({ chosen: selection.chosen, move, offer });
  const moves: Moves = {
    selected: selection.selected,
    carrying: dragging.carrying,
    over: dragging.over,
    pick: selection.pick,
    drag: dragging.drag,
    dragEnd: dragging.dragEnd,
    takes: dragging.takes,
    moveTo: (item) => ask({ to: 'move', items: selection.chosen(item) }),
  };
  // Where files from the computer go: the folder under the pointer, or the folder open.
  const over = dragging.over;
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
        className={`documents-drop${dragging.filesOver ? ' dragging' : ''}`}
        onDragOver={(event) => {
          if (dragging.carries() || !event.dataTransfer.types.includes('Files')) return;
          event.preventDefault();
          dragging.setFilesOver(true);
        }}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null))
            dragging.setFilesOver(false);
        }}
        onDrop={(event) => {
          if (dragging.carries() || !event.dataTransfer.types.includes('Files')) return;
          event.preventDefault();
          dragging.setFilesOver(false);
          void dropped(event.dataTransfer).then((files) => offer(files));
        }}
        onClick={(event) => {
          // A click beside everything lets go of the selection.
          if (selection.selected.size && !(event.target as Element).closest('[data-item]'))
            selection.clear();
        }}
      >
        {dragging.filesOver && (
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
                  reveal={changes.reveal}
                  revealed={changes.revealed}
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
