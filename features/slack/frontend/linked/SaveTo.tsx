import { Folder, FolderPlus } from 'lucide-react';
import type { SlackFolder } from '../../api/client.js';

// Where what's kept goes: a new folder named after the channel, or one chosen from where the DSP
// keeps its files, which choosing browses.

export function SaveTo({
  store,
  channel,
  folder,
  newFolder,
  browse,
}: {
  /** Where the DSP keeps its files, by name: "Documents". */
  store: string;
  /** The channel's name, once one is chosen. */
  channel: string | undefined;
  /** The folder chosen; none for a new one named after the channel. */
  folder: SlackFolder | undefined;
  newFolder: () => void;
  browse: () => void;
}) {
  return (
    <fieldset>
      <legend>Save to</legend>
      <ul className="slack-choices">
        <li>
          <label>
            <input type="radio" name="folder" checked={!folder} onChange={newFolder} />
            <FolderPlus size={15} />
            {channel ? `New folder “${channel}”` : 'New folder named after the channel'}
          </label>
        </li>
        <li>
          <label>
            <input type="radio" name="folder" checked={Boolean(folder)} onChange={browse} />
            <Folder size={15} />A folder in {store}
          </label>
          {folder && (
            <span className="slack-chosen">
              {folder.name} ·{' '}
              <button className="text-button" onClick={browse}>
                Change
              </button>
            </span>
          )}
        </li>
      </ul>
    </fieldset>
  );
}
