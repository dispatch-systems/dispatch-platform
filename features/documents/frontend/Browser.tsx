import { useEffect, useState, useSyncExternalStore, type ReactNode } from 'react';
import {
  ChevronRight,
  Ellipsis,
  ExternalLink,
  LayoutGrid,
  List,
  LoaderCircle,
  Plus,
  Users,
} from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { dspHash, hashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import {
  ConfirmDialog,
  DataState,
  Empty,
  ErrorBox,
  Modal,
  Popover,
  SearchInput,
} from '../../../core/shell/frontend/ui/index.js';
import {
  createItem,
  renameItem,
  trashItem,
  useDocumentsFolder,
  linkGoogle,
  type DocumentsConnection,
  type DocumentsFolder,
  type DocumentsOverview,
  type DocumentsItem,
  type NewKind,
} from '../api/client.js';
import { GoogleLogo } from './GoogleLogo.js';
import { Initials, Thumb, Tile, edited, editedInline, kindLabel, typeOf } from './items.js';
import { TeamPanel } from './TeamPanel.js';
import { ReadyDialog } from './ReadyDialog.js';

// The DSP's Documents, once Google is connected: a folder at a time, as a list or a grid,
// with what Documents makes. A file opens in Google, in a tab of its own.

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
function useSettled(value: string, ms = 300) {
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
  | { to: 'trash'; item: DocumentsItem };

export function Browser({
  view,
  overview,
  connection,
  canManage,
  connected,
  overviewChanged,
}: {
  view: DspView;
  overview: DocumentsOverview;
  connection: DocumentsConnection;
  canManage: boolean;
  /** Whether Google was connected just now, so the page offers some folders to start with. */
  connected: boolean;
  overviewChanged: () => void;
}) {
  const folder = useFolder();
  const [search, setSearch] = useState('');
  const query = useSettled(search);
  const [filter, setFilter] = useState<Filter>('all');
  const [layout, setLayout] = useState<Layout>(() =>
    localStorage.getItem(LAYOUT) === 'grid' ? 'grid' : 'list',
  );
  const listing = useDocumentsFolder(folder, query);
  const [asked, ask] = useState<Asked>();
  const [team, showTeam] = useState(false);
  const [ready, setReady] = useState(connected);
  // A listing Google refused may mean the connection broke: the overview says.
  useEffect(() => {
    if (listing.error) overviewChanged();
  }, [listing.error, overviewChanged]);
  useEffect(() => setSearch(''), [folder]);
  const choose = (next: Layout) => {
    setLayout(next);
    localStorage.setItem(LAYOUT, next);
  };
  const searching = Boolean(query.trim());
  const at = listing.data;
  const top = !folder;
  return (
    <>
      <div className="page-heading documents-heading">
        <div>
          <h1>Documents</h1>
          <p>
            Shared with everyone at {view.dsp.name}. Anything added here also shows up in Google
            Drive.
          </p>
        </div>
        <div className="heading-actions">
          {canManage && (
            <button className="documents-account-button" onClick={() => showTeam(true)}>
              <Users size={16} />
              {overview.editors === 0
                ? 'Team access'
                : overview.editors === 1
                  ? '1 person'
                  : `${overview.editors} people`}
            </button>
          )}
          <a
            className="documents-link-button"
            href={at?.url ?? connection.folderUrl}
            target="_blank"
            rel="noreferrer"
          >
            <ExternalLink size={16} />
            Open in Drive
          </a>
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
        </div>
      </div>
      <Linking overview={overview} />
      <div className="documents-toolbar">
        {at && at.path.length > 0 ? (
          <Path view={view} at={at} />
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
      <DataState data={listing.data} error={listing.error} retry={listing.refresh}>
        {(at) => {
          const items = at.items.filter(shown(filter));
          if (!items.length)
            return searching ? (
              <Empty title="Nothing found">
                No file or folder {top ? 'in Documents' : `in ${at.name}`} has that in its name.
              </Empty>
            ) : at.items.length ? (
              <Empty title={`No ${FILTERS.find(([value]) => value === filter)![1]} here`} />
            ) : (
              <Empty title={top ? 'Nothing here yet' : `${at.name} is empty`}>
                Use New to add a folder, a Google Doc, a Sheet or Slides.
              </Empty>
            );
          const props = { view, items, searching, ask };
          const folders = items.filter((item) => item.kind === 'folder').length;
          return (
            <>
              {layout === 'list' ? <Table {...props} /> : <Grid {...props} />}
              <p className="documents-count">{counted(folders, items.length - folders)}</p>
            </>
          );
        }}
      </DataState>
      {asked?.to === 'make' && (
        <Naming
          title={`New ${NEW.find(([kind]) => kind === asked.kind)![1]}`}
          initial={NEW.find(([kind]) => kind === asked.kind)![2]}
          confirm="Create"
          done={() => ask(undefined)}
          save={async (name) => {
            // A tab opened by the click itself, so no popup blocker stops it; it goes to the
            // file once Google has made it.
            const tab = asked.kind === 'folder' ? null : window.open('about:blank', '_blank');
            try {
              const made = await createItem(folder, asked.kind, name);
              if (tab) tab.location.href = made.url;
            } catch (error) {
              tab?.close();
              throw error;
            }
            listing.refresh();
          }}
          success={`${kindLabel(asked.kind)} created`}
        />
      )}
      {asked?.to === 'rename' && (
        <Naming
          title={`Rename ${asked.item.kind === 'folder' ? 'folder' : 'file'}`}
          initial={asked.item.name}
          confirm="Rename"
          done={() => ask(undefined)}
          save={async (name) => {
            await renameItem(asked.item.id, name);
            listing.refresh();
          }}
          success="Renamed"
        />
      )}
      {asked?.to === 'trash' && (
        <Trash item={asked.item} done={() => ask(undefined)} trashed={listing.refresh} />
      )}
      {team && (
        <TeamPanel
          view={view}
          connection={connection}
          close={() => {
            showTeam(false);
            overviewChanged();
          }}
          disconnected={overviewChanged}
        />
      )}
      {ready && top && at && !searching && at.items.length === 0 && (
        <ReadyDialog
          connection={connection}
          close={() => setReady(false)}
          made={() => {
            setReady(false);
            listing.refresh();
          }}
        />
      )}
    </>
  );
}

/** What the member asking needs to do to edit in Google, if anything. */
function Linking({ overview }: { overview: DocumentsOverview }) {
  const linking = useAction(async () => {
    const { url } = await linkGoogle();
    window.location.assign(url);
  });
  const me = overview.me;
  if (!me || me.state === 'shared') return null;
  return (
    <section className="documents-banner">
      <GoogleLogo size={20} />
      <div>
        {me.state === 'needs_account' ? (
          <>
            <strong>Link a Google account to edit Docs and Sheets</strong>
            <span>
              {me.email} isn't a Google account, so you can see your team's files here but can't
              edit them in Google yet. Any Gmail address works.
            </span>
          </>
        ) : (
          <>
            <strong>Google won't share your team's folder with {me.email}</strong>
            <span>Link another Google account, or ask your DSP owner.</span>
          </>
        )}
      </div>
      <button disabled={linking.busy} onClick={() => void linking.run()}>
        Link Google account
      </button>
    </section>
  );
}

const count = (n: number, noun: string) => `${n} ${noun}${n === 1 ? '' : 's'}`;
/** "7 folders and 8 files", or only the part there is. */
const counted = (folders: number, files: number) =>
  [folders && count(folders, 'folder'), files && count(files, 'file')]
    .filter(Boolean)
    .join(' and ');

/** The way from the top of Documents to the folder open now. */
function Path({ view, at }: { view: DspView; at: DocumentsFolder }) {
  return (
    <nav className="documents-path" aria-label="Folder">
      <a href={dspHash(view.dsp.id, 'documents')}>Documents</a>
      {at.path.map((step, index) => (
        <span key={step.id}>
          <ChevronRight size={14} aria-hidden="true" />
          {index === at.path.length - 1 ? (
            <strong aria-current="page">{step.name}</strong>
          ) : (
            <a href={dspHash(view.dsp.id, 'documents', { folder: step.id })}>{step.name}</a>
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
};

/** Where an item opens: a folder in Documents, a file in Google, in a tab of its own. */
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
  return item.kind === 'folder' ? (
    <a className={className} href={dspHash(view.dsp.id, 'documents', { folder: item.id })}>
      {children}
    </a>
  ) : (
    <a className={className} href={item.url} target="_blank" rel="noreferrer">
      {children}
    </a>
  );
}

function Actions({ item, ask }: { item: DocumentsItem; ask: (asked: Asked) => void }) {
  return (
    <Popover
      className="row-menu"
      label={`Actions for ${item.name}`}
      trigger={<Ellipsis size={18} />}
      anchored
    >
      {item.kind !== 'folder' && (
        <a href={item.url} target="_blank" rel="noreferrer">
          <ExternalLink size={14} />
          Open in Google
        </a>
      )}
      <button onClick={() => ask({ to: 'rename', item })}>Rename</button>
      <button className="danger" onClick={() => ask({ to: 'trash', item })}>
        Move to trash
      </button>
    </Popover>
  );
}

function Table({ view, items, searching, ask }: ViewProps) {
  const zone = view.dsp.timezone;
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
            <tr key={item.id}>
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
                <span className="documents-edited">
                  {edited(item.modifiedAt, zone)}
                  {item.modifiedBy && <small>{item.modifiedBy}</small>}
                </span>
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
                <Actions item={item} ask={ask} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Grid({ view, items, searching, ask }: ViewProps) {
  const zone = view.dsp.timezone;
  const folders = items.filter((item) => item.kind === 'folder');
  const files = items.filter((item) => item.kind !== 'folder');
  return (
    <div className="documents-cards">
      {folders.length > 0 && (
        <section aria-label="Folders">
          <h2>Folders</h2>
          <div className="documents-card-grid">
            {folders.map((item) => (
              <div className="documents-folder-card" key={item.id}>
                <Opens view={view} item={item} className="documents-name">
                  <Tile kind="folder" size={18} />
                  <span>
                    <strong>{item.name}</strong>
                    <small>{searching ? `in ${item.location ?? 'Documents'}` : typeOf(item)}</small>
                  </span>
                </Opens>
                <Actions item={item} ask={ask} />
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
              <div className="documents-file-card" key={item.id}>
                <Opens view={view} item={item} className="documents-file-open">
                  <Thumb kind={item.kind} />
                  <span className="documents-name">
                    <Tile kind={item.kind} />
                    <span>
                      <strong>{item.name}</strong>
                      <small>
                        {searching
                          ? `in ${item.location ?? 'Documents'}`
                          : `Edited ${editedInline(item.modifiedAt, zone)}${item.modifiedBy ? ` by ${item.modifiedBy}` : ''}`}
                      </small>
                    </span>
                  </span>
                </Opens>
                <Actions item={item} ask={ask} />
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

/** Asks for a name, to make something or rename it. */
function Naming({
  title,
  initial,
  confirm,
  save,
  success,
  done,
}: {
  title: string;
  initial: string;
  confirm: string;
  save: (name: string) => Promise<void>;
  success: string;
  done: () => void;
}) {
  const [name, setName] = useState(initial);
  const saving = useAction(
    async () => {
      await save(name.trim());
      done();
    },
    { success, inline: true },
  );
  return (
    <Modal title={title} onClose={done} initialFocus="input">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void saving.run();
        }}
      >
        <label>
          Name
          <input
            value={name}
            maxLength={200}
            required
            onChange={(event) => setName(event.target.value)}
            onFocus={(event) => event.target.select()}
          />
        </label>
        <ErrorBox message={saving.error} />
        <div className="form-actions">
          <button type="button" onClick={done}>
            Cancel
          </button>
          <button className="primary" disabled={saving.busy || !name.trim()}>
            {saving.busy && <LoaderCircle className="spin" size={16} />}
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
  trashed,
}: {
  item: DocumentsItem;
  done: () => void;
  trashed: () => void;
}) {
  const trashing = useAction(
    async () => {
      await trashItem(item.id);
      done();
      trashed();
    },
    { success: 'Moved to the trash' },
  );
  return (
    <ConfirmDialog
      title={`Move ${item.name} to the trash?`}
      confirm="Move to trash"
      tone="danger"
      busy={trashing.busy}
      onConfirm={() => void trashing.run()}
      onCancel={done}
    >
      {item.kind === 'folder'
        ? 'It and everything in it go to the trash of the Google account that holds Documents, for 30 days. Its owner can restore them from Google Drive.'
        : 'It goes to the trash of the Google account that holds Documents, for 30 days. Its owner can restore it from Google Drive.'}
    </ConfirmDialog>
  );
}
