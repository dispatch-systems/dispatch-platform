import { Settings2 } from 'lucide-react';
import type { SlackWorkspace } from '../../api/client.js';
import { Badge } from '../../../../core/shell/frontend/ui/index.js';
import { monthDay } from '../../../../core/shell/frontend/lib/format.js';
import { SlackCard } from './SlackCard.js';

// One of the DSP's workspaces, on its own card, with what the member may do to it: its settings,
// for those who manage Slack, where it's renamed too, and the rest for those who manage the DSP's
// connections.

export function WorkspaceCard({
  workspace,
  timeZone,
  manages,
  configures,
  busy,
  settings,
  install,
  disconnect,
}: {
  workspace: SlackWorkspace;
  timeZone: string;
  /** Whether the member manages the DSP's connections. */
  manages: boolean;
  /** Whether the member manages Slack, and so its settings. */
  configures: boolean;
  /** Whether an install is under way, which a reconnect or update waits for. */
  busy: boolean;
  /** Where its settings are, for those who manage Slack. */
  settings: string;
  /** Reconnects or updates the workspace `team`. */
  install: (team: string) => void;
  disconnect: (workspace: SlackWorkspace) => void;
}) {
  const broken = workspace.status === 'broken';
  return (
    <SlackCard
      title={workspace.label ?? workspace.name}
      actions={
        <>
          {manages && (broken || workspace.needsUpdate) && (
            <button className="primary" disabled={busy} onClick={() => install(workspace.teamId)}>
              {broken ? 'Reconnect' : 'Update'}
            </button>
          )}
          {configures && (
            <a className="slack-link-button" href={settings}>
              <Settings2 size={16} />
              Settings
            </a>
          )}
          {manages && (
            <button className="text-button" onClick={() => disconnect(workspace)}>
              Disconnect
            </button>
          )}
        </>
      }
    >
      <div role="status">
        {broken ? (
          <Badge value="failed">Needs Reconnecting</Badge>
        ) : workspace.needsUpdate ? (
          <Badge value="needs_verification">Needs Update</Badge>
        ) : (
          <Badge value="ready" />
        )}
      </div>
      <dl className="slack-details">
        <dt>Workspace</dt>
        <dd>{workspace.name}</dd>
        <dt>Connected</dt>
        <dd>
          {monthDay(workspace.connectedAt, timeZone)}
          {workspace.byLink
            ? ', by install link'
            : workspace.connectedBy
              ? `, by ${workspace.connectedBy}`
              : ''}
        </dd>
      </dl>
    </SlackCard>
  );
}
