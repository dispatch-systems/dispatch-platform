import type { SlackWorkspace } from '../api/client.js';
import { Badge } from '../../../core/shell/frontend/ui/index.js';
import { day } from './day.js';

// The DSP's workspaces on its Slack card, each with what can be done to it.

export function Workspaces({
  workspaces,
  timeZone,
  busy,
  install,
  disconnect,
}: {
  workspaces: readonly SlackWorkspace[];
  timeZone: string;
  /** Whether an install is under way, which a reconnect or update waits for. */
  busy: boolean;
  /** Reconnects or updates the workspace `team`. */
  install: (team: string) => void;
  disconnect: (workspace: SlackWorkspace) => void;
}) {
  return (
    <ul className="slack-workspaces">
      {workspaces.map((workspace) => {
        const broken = workspace.status === 'broken';
        return (
          <li key={workspace.teamId}>
            <div className="slack-workspace-name">
              <strong>{workspace.name}</strong>
              {broken ? (
                <Badge value="failed">Needs Reconnecting</Badge>
              ) : workspace.needsUpdate ? (
                <Badge value="needs_verification">Needs Update</Badge>
              ) : (
                <Badge value="ready" />
              )}
            </div>
            <small>
              {day(workspace.connectedAt, timeZone)}
              {workspace.byLink
                ? ', by install link'
                : workspace.connectedBy
                  ? `, by ${workspace.connectedBy}`
                  : ''}
            </small>
            <div className="slack-workspace-actions">
              {(broken || workspace.needsUpdate) && (
                <button
                  className="primary"
                  disabled={busy}
                  onClick={() => install(workspace.teamId)}
                >
                  {broken ? 'Reconnect' : 'Update'}
                </button>
              )}
              <button className="text-button" onClick={() => disconnect(workspace)}>
                Disconnect
              </button>
            </div>
          </li>
        );
      })}
    </ul>
  );
}
