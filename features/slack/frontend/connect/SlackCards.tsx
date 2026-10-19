import { useState } from 'react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { settingsHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Badge, ConfirmDialog, DataState } from '../../../../core/shell/frontend/ui/index.js';
import {
  connectSlack,
  disconnectSlack,
  finishSlack,
  useSlackAccount,
  type SlackWorkspace,
} from '../../api/client.js';
import { LinkCard } from './InstallLink.js';
import { SlackCard } from './SlackCard.js';
import { useSlackReturn } from './slackReturn.js';
import { WorkspaceCard } from './WorkspaceCard.js';

// Slack's cards on Settings' DSP Connections: one for each of the DSP's workspaces, and for
// those who manage the DSP's connections, one for its install link while it works and one for an
// install Slack just sent the browser back from. New there adds a workspace. Those who manage
// Slack, alone, see the workspaces' cards with a link to each one's settings, on Slack's page of
// Settings.

export function SlackCards({ view }: { view: DspView }) {
  const manages = can(view, 'connections.manage');
  const configures = can(view, 'slack.manage');
  const account = useSlackAccount();
  const timeZone = view.dsp.timezone;
  const returned = useSlackReturn(async (state, code) => {
    // One refused may still have changed: a workspace added again keeps its new tokens.
    try {
      await finishSlack(state, code);
    } finally {
      account.refresh();
    }
  });
  // To Slack, to reconnect or update the workspace `team`.
  const install = useAction(async (team: string) => {
    const { url } = await connectSlack({ team });
    window.location.assign(url);
  });
  const [leaving, setLeaving] = useState<SlackWorkspace>();
  const disconnect = useAction(
    async (team: string) => {
      await disconnectSlack(team);
      account.refresh();
      setLeaving(undefined);
    },
    { success: 'Slack disconnected' },
  );
  return (
    <>
      <DataState data={account.data} error={account.error} retry={account.refresh}>
        {(data) => (
          <>
            {returned.busy && (
              <SlackCard title="Slack">
                <div role="status">
                  <Badge value="signing_in">Connecting</Badge>
                </div>
              </SlackCard>
            )}
            {data.workspaces.map((workspace) => (
              <WorkspaceCard
                key={workspace.teamId}
                workspace={workspace}
                timeZone={timeZone}
                manages={manages}
                configures={configures}
                busy={install.busy || returned.busy}
                settings={settingsHash(view.dsp.id, 'slack', view, {
                  workspace: workspace.teamId,
                })}
                install={(team) => void install.run(team)}
                disconnect={setLeaving}
              />
            ))}
            {manages && data.link && <LinkCard link={data.link} timeZone={timeZone} />}
          </>
        )}
      </DataState>
      {leaving && (
        <ConfirmDialog
          title={`Disconnect ${leaving.label ?? leaving.name}?`}
          confirm="Disconnect"
          tone="danger"
          busy={disconnect.busy}
          onConfirm={() => void disconnect.run(leaving.teamId)}
          onCancel={() => setLeaving(undefined)}
        >
          Dispatch is removed from {leaving.name} in Slack.
        </ConfirmDialog>
      )}
    </>
  );
}
