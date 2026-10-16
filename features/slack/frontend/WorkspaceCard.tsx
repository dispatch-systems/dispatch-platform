import type { SlackWorkspace } from '../api/client.js';
import { Badge } from '../../../core/shell/frontend/ui/index.js';
import { day } from './day.js';
import { SlackCard } from './SlackCard.js';

// One of the DSP's workspaces, on its own card, with what can be done to it.

export function WorkspaceCard({
  workspace,
  timeZone,
  busy,
  install,
  rename,
  disconnect,
}: {
  workspace: SlackWorkspace;
  timeZone: string;
  /** Whether an install is under way, which a reconnect or update waits for. */
  busy: boolean;
  /** Reconnects or updates the workspace `team`. */
  install: (team: string) => void;
  rename: (workspace: SlackWorkspace) => void;
  disconnect: (workspace: SlackWorkspace) => void;
}) {
  const broken = workspace.status === 'broken';
  return (
    <SlackCard
      title={workspace.label ?? workspace.name}
      actions={
        <>
          {(broken || workspace.needsUpdate) && (
            <button className="primary" disabled={busy} onClick={() => install(workspace.teamId)}>
              {broken ? 'Reconnect' : 'Update'}
            </button>
          )}
          <button className="text-button" onClick={() => rename(workspace)}>
            Rename
          </button>
          <button className="text-button" onClick={() => disconnect(workspace)}>
            Disconnect
          </button>
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
          {day(workspace.connectedAt, timeZone)}
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
