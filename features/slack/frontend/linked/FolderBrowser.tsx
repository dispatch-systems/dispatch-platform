import { useCallback, useState } from 'react';
import { ChevronRight, Folder } from 'lucide-react';
import { useData } from '../../../../core/shell/frontend/runtime/api.js';
import { DataState } from '../../../../core/shell/frontend/ui/index.js';
import type { SlackFolder, SlackFolders } from '../../api/client.js';
import type { Linking } from './linking.js';

// Choosing a folder where the DSP keeps its files: the folder open, with where it is above it and
// the folders it holds below, each opened by a click; then Use, for the one open.

export function FolderBrowser({
  team,
  linking,
  choose,
  back,
}: {
  team: string;
  linking: Linking;
  /** Once a folder is chosen. */
  choose: (folder: SlackFolder) => void;
  /** Goes back without choosing. */
  back: () => void;
}) {
  const [open, setOpen] = useState<string>();
  const url = linking.foldersUrl(team, open);
  const read = useCallback(
    (signal: AbortSignal) => linking.browse(team, open, signal),
    [linking, team, open],
  );
  const folders = useData<SlackFolders>(url, 0, undefined, undefined, false, read);
  return (
    <DataState data={folders.data} error={folders.error} retry={folders.refresh}>
      {(data) => {
        const here = data.path[data.path.length - 1];
        return (
          <div className="slack-folders">
            <nav className="slack-folder-path" aria-label="Folder">
              {data.path.map((step, index) => (
                <span key={step.id}>
                  {index > 0 && <span aria-hidden="true">›</span>}
                  {index < data.path.length - 1 ? (
                    <button className="text-button" onClick={() => setOpen(step.id)}>
                      {step.name}
                    </button>
                  ) : (
                    <b>{step.name}</b>
                  )}
                </span>
              ))}
            </nav>
            {data.folders.length > 0 && (
              <ul className="slack-folder-list" aria-label="Folders">
                {data.folders.map((folder) => (
                  <li key={folder.id}>
                    <button onClick={() => setOpen(folder.id)}>
                      <Folder size={15} aria-hidden="true" />
                      {folder.name}
                      <ChevronRight size={15} aria-hidden="true" />
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <div className="form-actions">
              <button onClick={back}>Back</button>
              {here && (
                <button className="primary" onClick={() => choose(here)}>
                  Use {here.name}
                </button>
              )}
            </div>
          </div>
        );
      }}
    </DataState>
  );
}
