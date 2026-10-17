import { useState } from 'react';
import { NotebookText } from 'lucide-react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { DataState, ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import {
  canvasLinking,
  chooseSlackCanvases,
  useSlackChannelCanvases,
  useSlackChannels,
  type SlackFolder,
} from '../../api/client.js';
import { ChannelChoice, NoChannels } from '../kept/ChannelChoice.js';
import { FolderBrowser } from '../kept/FolderBrowser.js';
import { SaveTo } from '../kept/SaveTo.js';

// Choosing canvases on their own: one of the channels Dispatch is in, the canvases in it to
// sync, and where their documents go, a new folder named after the channel or one chosen from
// where the DSP keeps its files, browsed in the same dialog.

export function ChooseCanvases({
  team,
  store,
  open,
  close,
}: {
  team: string;
  /** Where the DSP keeps its files, by name: "Documents". */
  store: string;
  /** Opens another of the workspace's sections, by its id. */
  open: (section: string) => void;
  close: () => void;
}) {
  const channels = useSlackChannels(team);
  const [channel, setChannel] = useState<string>();
  const [canvases, setCanvases] = useState<ReadonlySet<string>>(new Set());
  // A folder chosen; none for a new one named after the channel.
  const [folder, setFolder] = useState<SlackFolder>();
  const [browsing, setBrowsing] = useState(false);
  const choose = useAction(
    async () => {
      if (!channel || !canvases.size) return;
      await chooseSlackCanvases(team, channel, [...canvases], folder?.id);
      close();
    },
    { inline: true, success: canvases.size === 1 ? 'Canvas added' : 'Canvases added' },
  );
  if (browsing)
    return (
      <Modal title="Choose a folder" onClose={close} onBack={() => setBrowsing(false)}>
        <FolderBrowser
          team={team}
          linking={canvasLinking}
          back={() => setBrowsing(false)}
          choose={(chosen) => {
            setFolder(chosen);
            setBrowsing(false);
          }}
        />
      </Modal>
    );
  return (
    <Modal title="Choose canvases" onClose={close}>
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
                choose={(chosen) => {
                  setChannel(chosen);
                  setCanvases(new Set());
                }}
              />
              {channel && (
                <CanvasChoice
                  team={team}
                  channel={channel}
                  chosen={canvases}
                  choose={setCanvases}
                />
              )}
              <SaveTo
                store={store}
                channel={name}
                folder={folder}
                newFolder={() => setFolder(undefined)}
                browse={() => setBrowsing(true)}
              />
              <ErrorBox message={choose.error} />
              <div className="form-actions">
                <button onClick={close}>Cancel</button>
                <button
                  className="primary"
                  disabled={!canvases.size || choose.busy}
                  onClick={() => void choose.run()}
                >
                  Sync
                </button>
              </div>
            </div>
          );
        }}
      </DataState>
    </Modal>
  );
}

/** The canvases in the channel, each ticked to sync; those synced already shown as such. */
function CanvasChoice({
  team,
  channel,
  chosen,
  choose,
}: {
  team: string;
  channel: string;
  chosen: ReadonlySet<string>;
  choose: (chosen: ReadonlySet<string>) => void;
}) {
  const canvases = useSlackChannelCanvases(team, channel);
  const toggle = (id: string) => {
    const next = new Set(chosen);
    if (!next.delete(id)) next.add(id);
    choose(next);
  };
  return (
    <fieldset>
      <legend>Canvases</legend>
      <DataState data={canvases.data} error={canvases.error} retry={canvases.refresh}>
        {(data) =>
          data.canvases.length ? (
            <ul className="slack-choices">
              {data.canvases.map((canvas) => {
                const synced = data.linked || canvas.chosen;
                return (
                  <li key={canvas.id}>
                    <label className={synced ? 'taken' : undefined}>
                      <input
                        type="checkbox"
                        checked={synced || chosen.has(canvas.id)}
                        disabled={synced}
                        onChange={() => toggle(canvas.id)}
                      />
                      <NotebookText size={14} />
                      {canvas.title}
                      {synced && <span>Synced</span>}
                    </label>
                  </li>
                );
              })}
            </ul>
          ) : (
            <p className="slack-choices-none">No canvases in this channel.</p>
          )
        }
      </DataState>
    </fieldset>
  );
}
