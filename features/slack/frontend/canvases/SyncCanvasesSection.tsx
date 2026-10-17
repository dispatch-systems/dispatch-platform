import { useState } from 'react';
import { AlertCircle, ListChecks, NotebookText, Plus } from 'lucide-react';
import type { ConnectionSectionContext } from '../../../../core/shell/frontend/runtime/slots.js';
import { DataState } from '../../../../core/shell/frontend/ui/index.js';
import { canvasLinking, useSlackCanvases, type SlackCanvasLink } from '../../api/client.js';
import { ChannelLinks } from '../linked/ChannelLinks.js';
import { LinkChannel } from '../linked/LinkChannel.js';
import { ChooseCanvases } from './ChooseCanvases.js';
import { KeptCanvases } from './KeptCanvases.js';

// Sync Canvases' own section of a workspace's settings: the channels linked to folders, every
// canvas kept, linking another channel, and choosing canvases on their own. While documents
// can't be written, it says why, and canvases wait.

/** How many of a linked channel's canvases are kept, in a few words. */
const kept = (link: SlackCanvasLink) =>
  link.kept ? `${link.kept} ${link.kept === 1 ? 'canvas' : 'canvases'}` : 'No canvases yet';
const stop = {
  label: 'Stop syncing',
  done: 'Stopped syncing',
  stays: (folder: string) => `Documents already made stay in ${folder}.`,
};

export function SyncCanvasesSection({ view, account: team, open }: ConnectionSectionContext) {
  const canvases = useSlackCanvases(team);
  const [adding, setAdding] = useState<'channel' | 'canvases'>();
  const timeZone = view?.dsp.timezone ?? 'UTC';
  return (
    <DataState data={canvases.data} error={canvases.error} retry={canvases.refresh}>
      {(data) => {
        const any = data.links.length > 0 || data.canvases.length > 0;
        const add = (disabled: boolean) => (
          <>
            <button disabled={disabled} onClick={() => setAdding('canvases')}>
              <ListChecks size={16} aria-hidden="true" />
              Choose canvases
            </button>
            <button className="primary" disabled={disabled} onClick={() => setAdding('channel')}>
              <Plus size={16} aria-hidden="true" />
              Link a channel
            </button>
          </>
        );
        return (
          <div className="slack-kept">
            {any && <div className="slack-section-actions">{add(false)}</div>}
            {data.unready && (
              <div className="slack-kept-unready" role="status">
                <AlertCircle size={16} aria-hidden="true" />
                {data.unready}. Canvases wait until it is.
              </div>
            )}
            {data.links.length > 0 && (
              <ChannelLinks
                team={team}
                linking={canvasLinking}
                links={data.links}
                kept={kept}
                stop={stop}
              />
            )}
            {data.canvases.length > 0 && (
              <KeptCanvases team={team} canvases={data.canvases} timeZone={timeZone} />
            )}
            {!any && (
              <div className="slack-kept-empty">
                <span>
                  <NotebookText size={22} aria-hidden="true" />
                </span>
                <b>No canvases synced</b>
                <div className="slack-kept-empty-actions">{add(Boolean(data.unready))}</div>
              </div>
            )}
            {adding === 'channel' && (
              <LinkChannel
                team={team}
                linking={canvasLinking}
                store={data.store}
                linked={data.links}
                open={open}
                close={() => setAdding(undefined)}
              />
            )}
            {adding === 'canvases' && (
              <ChooseCanvases
                team={team}
                store={data.store}
                open={open}
                close={() => setAdding(undefined)}
              />
            )}
          </div>
        );
      }}
    </DataState>
  );
}
