import { useState } from 'react';
import { AlertCircle, ArrowRight, Ellipsis, Folder, Hash, Lock } from 'lucide-react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ConfirmDialog, Modal, Popover } from '../../../../core/shell/frontend/ui/index.js';
import { FolderBrowser } from './FolderBrowser.js';
import type { LinkedChannel, Linking } from './linking.js';

// A workspace's channels linked for one of Slack's integrations, each with the folder what it
// keeps goes to and what it's kept, and a menu to change its folder or stop.

export function ChannelLinks<Link extends LinkedChannel>({
  team,
  linking,
  links,
  kept,
  stop: stopping,
}: {
  team: string;
  linking: Linking;
  links: readonly Link[];
  /** What the channel has kept, in a few words. */
  kept: (link: Link) => string;
  /** Stopping: its action, what's said once it's done, and what's said of what stays. */
  stop: { label: string; done: string; stays: (folder: string) => string };
}) {
  const [moving, setMoving] = useState<Link>();
  const [confirming, setConfirming] = useState<Link>();
  const stop = useAction(
    async (link: Link) => {
      await linking.unlink(team, link.channelId);
      setConfirming(undefined);
    },
    { success: stopping.done },
  );
  const move = useAction(
    async (link: Link, folder: string) => {
      await linking.move(team, link.channelId, folder);
      setMoving(undefined);
    },
    { success: 'Folder changed' },
  );
  return (
    <div className="slack-channel-group">
      <h3>
        Channels
        <span className="slack-count">{links.length}</span>
      </h3>
      <ul className="slack-kept-list" aria-label="Linked channels">
        {links.map((link) => (
          <li key={link.channelId}>
            <div className="slack-kept-what">
              <b>
                {link.private ? <Lock size={14} /> : <Hash size={14} />}
                {link.channelName}
              </b>
              <span>{kept(link)}</span>
            </div>
            <ArrowRight className="slack-kept-arrow" size={15} aria-hidden="true" />
            <span className={`slack-folder${link.folderMissing ? ' missing' : ''}`}>
              {link.folderMissing ? <AlertCircle size={15} /> : <Folder size={15} />}
              {link.folderMissing ? 'Folder missing' : link.folder.name}
            </span>
            <Popover
              className="row-menu"
              label={`Actions for ${link.channelName}`}
              trigger={<Ellipsis size={18} />}
              anchored
            >
              <button onClick={() => setMoving(link)}>Change folder</button>
              <button className="danger" onClick={() => setConfirming(link)}>
                {stopping.label}
              </button>
            </Popover>
          </li>
        ))}
      </ul>
      {moving && (
        <Modal title="Choose a folder" onClose={() => setMoving(undefined)}>
          <FolderBrowser
            team={team}
            linking={linking}
            back={() => setMoving(undefined)}
            choose={(folder) => void move.run(moving, folder.id)}
          />
        </Modal>
      )}
      {confirming && (
        <ConfirmDialog
          title={`${stopping.label} #${confirming.channelName}?`}
          confirm={stopping.label}
          tone="danger"
          busy={stop.busy}
          onConfirm={() => void stop.run(confirming)}
          onCancel={() => setConfirming(undefined)}
        >
          {stopping.stays(confirming.folder.name)}
        </ConfirmDialog>
      )}
    </div>
  );
}
