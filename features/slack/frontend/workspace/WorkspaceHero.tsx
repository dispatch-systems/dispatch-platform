import { useState } from 'react';
import { Pencil } from 'lucide-react';
import { Badge } from '../../../../core/shell/frontend/ui/index.js';
import { slackAccountChanged, type SlackWorkspace } from '../../api/client.js';
import { monthDay } from '../../../../core/shell/frontend/lib/format.js';
import { RenameWorkspace } from './RenameWorkspace.js';
import { SlackLogo } from '../SlackLogo.js';

// The workspace at the top of its settings: its name, with a pencil to rename it for those who
// manage the DSP's connections, where it came from, and whether it works.

export function WorkspaceHero({
  workspace,
  timeZone,
  renames,
}: {
  workspace: SlackWorkspace;
  timeZone: string;
  /** Whether the member may rename it: they manage the DSP's connections. */
  renames: boolean;
}) {
  const [renaming, setRenaming] = useState(false);
  const broken = workspace.status === 'broken';
  return (
    <header className="slack-hero">
      <span className="slack-mark slack-hero-mark">
        <SlackLogo size={28} />
      </span>
      <div className="slack-hero-text">
        <div className="slack-hero-name">
          <h1>{workspace.label ?? workspace.name}</h1>
          {renames && (
            <button
              className="icon-button slack-rename"
              aria-label="Rename"
              title="Rename"
              onClick={() => setRenaming(true)}
            >
              <Pencil size={16} aria-hidden="true" />
            </button>
          )}
        </div>
        <dl>
          <div>
            <dt>Workspace</dt>
            <dd>{workspace.name}</dd>
          </div>
          <div>
            <dt>Connected</dt>
            <dd>
              {monthDay(workspace.connectedAt, timeZone)}
              {workspace.byLink
                ? ', by install link'
                : workspace.connectedBy
                  ? `, by ${workspace.connectedBy}`
                  : ''}
            </dd>
          </div>
        </dl>
      </div>
      <div className="slack-hero-status" role="status">
        {broken ? (
          <Badge value="failed">Needs Reconnecting</Badge>
        ) : workspace.needsUpdate ? (
          <Badge value="needs_verification">Needs Update</Badge>
        ) : (
          <Badge value="ready" />
        )}
      </div>
      {renaming && (
        <RenameWorkspace
          workspace={workspace}
          done={() => {
            slackAccountChanged();
            setRenaming(false);
          }}
          close={() => setRenaming(false)}
        />
      )}
    </header>
  );
}
