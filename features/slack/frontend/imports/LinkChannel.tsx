import { useState } from 'react';
import { Folder, FolderPlus, Hash, Lock } from 'lucide-react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { DataState, ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import {
  linkSlackChannel,
  useSlackChannels,
  type SlackFolder,
  type SlackImportLink,
} from '../../api/client.js';
import { FolderBrowser } from './FolderBrowser.js';

// Linking a channel to a folder: one of the channels Dispatch is in that isn't linked yet, and
// where its files go, a new folder named after it, or one chosen from where the DSP keeps its
// files, browsed in the same dialog.

export function LinkChannel({
  team,
  store,
  linked,
  open,
  close,
}: {
  team: string;
  /** Where the DSP keeps its files, by name: "Documents". */
  store: string;
  linked: readonly SlackImportLink[];
  /** Opens another of the workspace's sections, by its id. */
  open: (section: string) => void;
  close: () => void;
}) {
  const channels = useSlackChannels(team);
  const [channel, setChannel] = useState<string>();
  // A folder chosen; none for a new one named after the channel.
  const [folder, setFolder] = useState<SlackFolder>();
  const [browsing, setBrowsing] = useState(false);
  const link = useAction(
    async () => {
      if (!channel) return;
      await linkSlackChannel(team, channel, folder?.id);
      close();
    },
    { inline: true, success: 'Channel linked' },
  );
  if (browsing)
    return (
      <Modal title="Choose a folder" onClose={close} onBack={() => setBrowsing(false)}>
        <FolderBrowser
          team={team}
          back={() => setBrowsing(false)}
          choose={(chosen) => {
            setFolder(chosen);
            setBrowsing(false);
          }}
        />
      </Modal>
    );
  return (
    <Modal title="Link a channel" onClose={close}>
      <DataState data={channels.data} error={channels.error} retry={channels.refresh}>
        {(data) => {
          const taken = new Set(linked.map((each) => each.channelId));
          const joined = data.channels.filter((each) => each.member);
          const name = joined.find((each) => each.id === channel)?.name;
          if (!joined.length)
            return (
              <div className="slack-link-none">
                <p>Dispatch isn't in any channels yet.</p>
                <div className="form-actions">
                  <button onClick={close}>Cancel</button>
                  <button
                    className="primary"
                    onClick={() => {
                      close();
                      open('channels');
                    }}
                  >
                    Open Channels
                  </button>
                </div>
              </div>
            );
          return (
            <div className="slack-link-channel">
              <fieldset>
                <legend>Channel</legend>
                <ul className="slack-choices">
                  {joined.map((each) => {
                    const isLinked = taken.has(each.id);
                    return (
                      <li key={each.id}>
                        <label className={isLinked ? 'taken' : undefined}>
                          <input
                            type="radio"
                            name="channel"
                            checked={channel === each.id}
                            disabled={isLinked}
                            onChange={() => setChannel(each.id)}
                          />
                          {each.private ? <Lock size={14} /> : <Hash size={14} />}
                          {each.name}
                          {isLinked && <span>Linked</span>}
                        </label>
                      </li>
                    );
                  })}
                </ul>
              </fieldset>
              <fieldset>
                <legend>Save to</legend>
                <ul className="slack-choices">
                  <li>
                    <label>
                      <input
                        type="radio"
                        name="folder"
                        checked={!folder}
                        onChange={() => setFolder(undefined)}
                      />
                      <FolderPlus size={15} />
                      {name ? `New folder “${name}”` : 'New folder named after the channel'}
                    </label>
                  </li>
                  <li>
                    <label>
                      <input
                        type="radio"
                        name="folder"
                        checked={Boolean(folder)}
                        onChange={() => setBrowsing(true)}
                      />
                      <Folder size={15} />A folder in {store}
                    </label>
                    {folder && (
                      <span className="slack-chosen">
                        {folder.name} ·{' '}
                        <button className="text-button" onClick={() => setBrowsing(true)}>
                          Change
                        </button>
                      </span>
                    )}
                  </li>
                </ul>
              </fieldset>
              <ErrorBox message={link.error} />
              <div className="form-actions">
                <button onClick={close}>Cancel</button>
                <button
                  className="primary"
                  disabled={!channel || link.busy}
                  onClick={() => void link.run()}
                >
                  Link
                </button>
              </div>
            </div>
          );
        }}
      </DataState>
    </Modal>
  );
}
