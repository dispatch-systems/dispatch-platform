import { useState } from 'react';
import { AlertCircle, Check, Clock, Ellipsis, Folder, NotebookText, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { time } from '../../../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../../../core/shell/frontend/runtime/useAction.js';
import { ConfirmDialog, Popover } from '../../../../../core/shell/frontend/ui/index.js';
import { type SlackCanvas } from '../../../api/client.js';
import { unchooseSlackCanvas } from './client.js';

// Every canvas Sync Canvases keeps, with its channel, and the folder it was chosen for on its
// own; how syncing it goes, and where its document opens; and, for one chosen on its own, a menu
// to stop syncing it.

export function KeptCanvases({
  team,
  canvases,
  timeZone,
}: {
  team: string;
  canvases: readonly SlackCanvas[];
  timeZone: string;
}) {
  const [stopping, setStopping] = useState<SlackCanvas>();
  const stop = useAction(
    async (canvas: SlackCanvas) => {
      await unchooseSlackCanvas(team, canvas.canvasId);
      setStopping(undefined);
    },
    { success: 'Stopped syncing' },
  );
  return (
    <div className="slack-channel-group">
      <h3>
        Canvases
        <span className="slack-count">{canvases.length}</span>
      </h3>
      <ul className="slack-kept-list" aria-label="Canvases">
        {canvases.map((canvas) => (
          <li key={canvas.canvasId}>
            <span className="slack-kept-kind">
              <NotebookText size={16} aria-hidden="true" />
            </span>
            <div className="slack-kept-what">
              <b>{canvas.title}</b>
              <span>
                {canvas.channelName && `#${canvas.channelName}`}
                {canvas.channelName && canvas.folder && ' · '}
                {canvas.folder && (
                  <span className="slack-kept-in">
                    <Folder size={12} aria-hidden="true" />
                    {canvas.folder.name}
                  </span>
                )}
              </span>
            </div>
            {canvas.syncedAt && (
              <span className="slack-kept-when">{time(canvas.syncedAt, timeZone)}</span>
            )}
            {status(canvas)}
            {canvas.link && (
              <a className="slack-kept-open" href={canvas.link} target="_blank" rel="noreferrer">
                Open
              </a>
            )}
            {canvas.folder && (
              <Popover
                className="row-menu"
                label={`Actions for ${canvas.title}`}
                trigger={<Ellipsis size={18} />}
                anchored
              >
                <button className="danger" onClick={() => setStopping(canvas)}>
                  Stop syncing
                </button>
              </Popover>
            )}
          </li>
        ))}
      </ul>
      {stopping && (
        <ConfirmDialog
          title={`Stop syncing ${stopping.title}?`}
          confirm="Stop syncing"
          tone="danger"
          busy={stop.busy}
          onConfirm={() => void stop.run(stopping)}
          onCancel={() => setStopping(undefined)}
        >
          {stopping.link
            ? `Its document stays in ${stopping.folder?.name ?? 'its folder'}.`
            : "It won't be synced."}
        </ConfirmDialog>
      )}
    </div>
  );
}

/** How syncing the canvas goes, in words: a folder gone matters only while it has no document. */
function status(canvas: SlackCanvas): ReactNode {
  if (canvas.folderMissing && !canvas.link)
    return (
      <span className="slack-kept-status left">
        <AlertCircle size={14} aria-hidden="true" />
        Folder missing
      </span>
    );
  switch (canvas.status) {
    case 'synced':
      return (
        <span className="slack-kept-status saved">
          <Check size={14} aria-hidden="true" />
          Synced
        </span>
      );
    case 'waiting':
      return (
        <span className="slack-kept-status waiting">
          <Clock size={14} aria-hidden="true" />
          {canvas.retrying ? 'Retrying' : canvas.link ? 'Updating' : 'Syncing'}
        </span>
      );
    case 'gone':
      return (
        <span className="slack-kept-status waiting">
          <Trash2 size={14} aria-hidden="true" />
          Deleted in Slack
        </span>
      );
    case 'failed':
      return (
        <span className="slack-kept-status left">
          <AlertCircle size={14} aria-hidden="true" />
          Couldn't sync
        </span>
      );
  }
}
