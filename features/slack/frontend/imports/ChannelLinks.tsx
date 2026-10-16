import { useState } from 'react';
import { AlertCircle, ArrowRight, Ellipsis, Folder, Hash, Lock } from 'lucide-react';
import { time } from '../../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ConfirmDialog, Modal, Popover } from '../../../../core/shell/frontend/ui/index.js';
import { moveSlackLink, unlinkSlackChannel, type SlackImportLink } from '../../api/client.js';
import { FolderBrowser } from './FolderBrowser.js';

// The workspace's linked channels, each with the folder its files go to and what it's saved, and
// a menu to change its folder or stop importing it.

export function ChannelLinks({
  team,
  links,
  timeZone,
}: {
  team: string;
  links: readonly SlackImportLink[];
  timeZone: string;
}) {
  const [moving, setMoving] = useState<SlackImportLink>();
  const [stopping, setStopping] = useState<SlackImportLink>();
  const stop = useAction(
    async (link: SlackImportLink) => {
      await unlinkSlackChannel(team, link.channelId);
      setStopping(undefined);
    },
    { success: 'Stopped importing' },
  );
  const move = useAction(
    async (link: SlackImportLink, folder: string) => {
      await moveSlackLink(team, link.channelId, folder);
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
      <ul className="slack-import-links" aria-label="Linked channels">
        {links.map((link) => (
          <li key={link.channelId}>
            <div className="slack-import-what">
              <b>
                {link.private ? <Lock size={14} /> : <Hash size={14} />}
                {link.channelName}
              </b>
              <span>
                {link.saved
                  ? `${link.saved} ${link.saved === 1 ? 'file' : 'files'} · last ${time(link.lastSavedAt, timeZone)}`
                  : 'No files yet'}
              </span>
            </div>
            <ArrowRight className="slack-import-arrow" size={15} aria-hidden="true" />
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
              <button className="danger" onClick={() => setStopping(link)}>
                Stop importing
              </button>
            </Popover>
          </li>
        ))}
      </ul>
      {moving && (
        <Modal title="Choose a folder" onClose={() => setMoving(undefined)}>
          <FolderBrowser
            team={team}
            back={() => setMoving(undefined)}
            choose={(folder) => void move.run(moving, folder.id)}
          />
        </Modal>
      )}
      {stopping && (
        <ConfirmDialog
          title={`Stop importing #${stopping.channelName}?`}
          confirm="Stop importing"
          tone="danger"
          busy={stop.busy}
          onConfirm={() => void stop.run(stopping)}
          onCancel={() => setStopping(undefined)}
        >
          Files already saved stay in {stopping.folder.name}.
        </ConfirmDialog>
      )}
    </div>
  );
}
