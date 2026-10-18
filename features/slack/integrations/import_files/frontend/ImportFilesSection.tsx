import { useState } from 'react';
import { AlertCircle, FolderInput, Plus } from 'lucide-react';
import type { ConnectionSectionContext } from '../../../../../core/shell/frontend/runtime/slots.js';
import { DataState } from '../../../../../core/shell/frontend/ui/index.js';
import { time } from '../../../../../core/shell/frontend/lib/format.js';
import { type SlackImportLink } from '../../../api/client.js';
import { importLinking, useSlackImports } from './client.js';
import { ChannelLinks } from '../../../frontend/linked/ChannelLinks.js';
import { LinkChannel } from '../../../frontend/linked/LinkChannel.js';
import { RecentImports } from './RecentImports.js';

/** What a linked channel has saved, in a few words. */
const saved = (timeZone: string) => (link: SlackImportLink) =>
  link.saved
    ? `${link.saved} ${link.saved === 1 ? 'file' : 'files'} · last ${time(link.lastSavedAt, timeZone)}`
    : 'No files yet';
const stop = {
  label: 'Stop importing',
  done: 'Stopped importing',
  stays: (folder: string) => `Files already saved stay in ${folder}.`,
};

// Import Files' own section of a workspace's settings: the channels linked to folders, linking
// another, and the files saved latest. While files can't be saved, it says why, and new ones
// wait.

export function ImportFilesSection({ view, account: team, open }: ConnectionSectionContext) {
  const imports = useSlackImports(team);
  const [linking, setLinking] = useState(false);
  const timeZone = view?.dsp.timezone ?? 'UTC';
  return (
    <DataState data={imports.data} error={imports.error} retry={imports.refresh}>
      {(data) => (
        <div className="slack-kept">
          {data.links.length > 0 && (
            <div className="slack-section-actions">
              <button className="primary" onClick={() => setLinking(true)}>
                <Plus size={16} aria-hidden="true" />
                Link a channel
              </button>
            </div>
          )}
          {data.unready && (
            <div className="slack-kept-unready" role="status">
              <AlertCircle size={16} aria-hidden="true" />
              {data.unready}. New files wait until it is.
            </div>
          )}
          {data.links.length ? (
            <ChannelLinks
              team={team}
              linking={importLinking}
              links={data.links}
              kept={saved(timeZone)}
              stop={stop}
            />
          ) : (
            <div className="slack-kept-empty">
              <span>
                <FolderInput size={22} aria-hidden="true" />
              </span>
              <b>No channels linked</b>
              <button
                className="primary"
                disabled={Boolean(data.unready)}
                onClick={() => setLinking(true)}
              >
                <Plus size={16} aria-hidden="true" />
                Link a channel
              </button>
            </div>
          )}
          {data.recent.length > 0 && <RecentImports files={data.recent} timeZone={timeZone} />}
          {linking && (
            <LinkChannel
              team={team}
              linking={importLinking}
              store={data.store}
              linked={data.links}
              open={open}
              close={() => setLinking(false)}
            />
          )}
        </div>
      )}
    </DataState>
  );
}
