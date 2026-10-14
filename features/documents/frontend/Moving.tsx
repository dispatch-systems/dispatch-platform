import { useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { DataState, Modal } from '../../../core/shell/frontend/ui/index.js';
import {
  prefetchFolder,
  useDocumentsFolder,
  type DocumentsItem,
  type FolderStep,
} from '../api/client.js';
import { Tile } from './items.js';

// Moving files and folders: dragged onto a folder or a step of the path, or with Move to…,
// which needs no pointer.

/** Where something moves: a folder, or the top, with the name it shows. */
export type Place = { id?: string; name: string };

/**
 * Picks the folder `items` move into, starting at the folder open: a folder opens on a click,
 * the path goes back up, and Move here moves them. One being moved opens nowhere, so no folder
 * goes inside itself.
 */
export function MoveTo({
  items,
  path: start,
  from,
  move,
  done,
}: {
  items: DocumentsItem[];
  /** The way to the folder open. */
  path: FolderStep[];
  /** The folder they're all in, `null` for the top, when it's known. */
  from?: string | null;
  move: (to: Place) => void;
  done: () => void;
}) {
  const [path, setPath] = useState(start);
  const at = path.at(-1);
  const listing = useDocumentsFolder(at?.id);
  const moving = new Set(items.map((item) => item.id));
  const here = from !== undefined && (at?.id ?? null) === from;
  return (
    <Modal
      title={items.length === 1 ? `Move ${items[0]!.name}` : `Move ${items.length} items`}
      onClose={done}
    >
      <nav className="documents-path documents-move-path" aria-label="Folder">
        {path.length ? (
          <button type="button" onClick={() => setPath([])}>
            Documents
          </button>
        ) : (
          <strong aria-current="page">Documents</strong>
        )}
        {path.map((step, index) => (
          <span key={step.id}>
            <ChevronRight size={14} aria-hidden="true" />
            {index === path.length - 1 ? (
              <strong aria-current="page">{step.name}</strong>
            ) : (
              <button type="button" onClick={() => setPath(path.slice(0, index + 1))}>
                {step.name}
              </button>
            )}
          </span>
        ))}
      </nav>
      <div className="documents-move-folders">
        <DataState
          data={listing.data ?? listing.stale}
          error={listing.error}
          retry={listing.refresh}
        >
          {(read) => {
            // Until the folder comes, the last one shows, faded, and opens nothing.
            const coming = !listing.data;
            const folders = read.items.filter((item) => item.kind === 'folder');
            if (!folders.length) return <p className="muted">No folders</p>;
            return (
              <ul className={coming ? 'documents-coming' : undefined}>
                {folders.map((folder) => (
                  <li key={folder.id}>
                    <button
                      type="button"
                      disabled={coming || moving.has(folder.id)}
                      onPointerEnter={() => prefetchFolder(folder.id)}
                      onClick={() => setPath([...path, { id: folder.id, name: folder.name }])}
                    >
                      <Tile kind="folder" />
                      <span>{folder.name}</span>
                      <ChevronRight size={16} aria-hidden="true" />
                    </button>
                  </li>
                ))}
              </ul>
            );
          }}
        </DataState>
      </div>
      <div className="form-actions">
        <button type="button" onClick={done}>
          Cancel
        </button>
        <button
          className="primary"
          disabled={here}
          onClick={() => {
            move(at ? { id: at.id, name: at.name } : { name: 'Documents' });
            done();
          }}
        >
          Move here
        </button>
      </div>
    </Modal>
  );
}

/** Shows `count` things as what the pointer carries, in place of the one the drag started on. */
export function carryCount(transfer: DataTransfer, count: number) {
  const ghost = document.createElement('div');
  ghost.className = 'documents-drag-count';
  ghost.textContent = `${count} items`;
  document.body.append(ghost);
  transfer.setDragImage(ghost, 16, 16);
  // The browser draws it as the drag starts, so it can go straight after.
  setTimeout(() => ghost.remove());
}
