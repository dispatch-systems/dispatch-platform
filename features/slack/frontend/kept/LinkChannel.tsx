import { useState } from 'react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { DataState, ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useSlackChannels, type SlackFolder } from '../../api/client.js';
import { ChannelChoice, NoChannels } from './ChannelChoice.js';
import { FolderBrowser } from './FolderBrowser.js';
import type { LinkedChannel, Linking } from './linking.js';
import { SaveTo } from './SaveTo.js';

// Linking a channel to a folder, for one of Slack's integrations: one of the channels Dispatch is
// in that isn't linked yet, and where what it keeps goes, a new folder named after the channel,
// or one chosen from where the DSP keeps its files, browsed in the same dialog.

export function LinkChannel({
  team,
  linking,
  store,
  linked,
  open,
  close,
}: {
  team: string;
  linking: Linking;
  /** Where the DSP keeps its files, by name: "Documents". */
  store: string;
  linked: readonly LinkedChannel[];
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
      await linking.link(team, channel, folder?.id);
      close();
    },
    { inline: true, success: 'Channel linked' },
  );
  if (browsing)
    return (
      <Modal title="Choose a folder" onClose={close} onBack={() => setBrowsing(false)}>
        <FolderBrowser
          team={team}
          linking={linking}
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
          const joined = data.channels.filter((each) => each.member);
          const name = joined.find((each) => each.id === channel)?.name;
          if (!joined.length) return <NoChannels open={open} close={close} />;
          return (
            <div className="slack-link-channel">
              <ChannelChoice
                channels={joined}
                chosen={channel}
                choose={setChannel}
                taken={new Set(linked.map((each) => each.channelId))}
              />
              <SaveTo
                store={store}
                channel={name}
                folder={folder}
                newFolder={() => setFolder(undefined)}
                browse={() => setBrowsing(true)}
              />
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
