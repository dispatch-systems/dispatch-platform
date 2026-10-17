import {
  useEffect,
  useState,
  type DragEvent as ReactDragEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from 'react';
import { ChevronRight, Download, Ellipsis, ExternalLink } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { dspHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Pagination, Popover, openContextMenu } from '../../../../core/shell/frontend/ui/index.js';
import {
  downloadItem,
  prefetchFolder,
  type DocumentsItem,
  type FolderStep,
} from '../../api/client.js';
import { Initials, Thumb, Tile, editedLabels, typeOf } from '../items.js';
import type { Place } from '../Moving.js';
import { look, making, type Asked, type Layout } from './browsing.js';

// What the browser draws: the way to the folder open, and the folder's items as a list or a
// grid, each row or card selectable, draggable and with its menu.

/** The way from the top of Documents to the folder open now: each step above it takes drops. */
export function Path({ view, path, moves }: { view: DspView; path: FolderStep[]; moves: Moves }) {
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

export type ViewProps = {
  view: DspView;
  items: DocumentsItem[];
  searching: boolean;
  ask: (asked: Asked) => void;
  moves: Moves;
};

/** Selecting, dragging and dropping, as the page keeps them. */
export type Moves = {
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
export function Listing({
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
