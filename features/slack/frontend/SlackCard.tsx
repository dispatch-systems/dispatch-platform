import { useState } from 'react';
import { Copy, LoaderCircle } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import {
  Badge,
  ConfirmDialog,
  DataState,
  ErrorBox,
} from '../../../core/shell/frontend/ui/index.js';
import {
  connectSlack,
  disconnectSlack,
  finishSlack,
  useSlackAccount,
  type SlackWorkspace,
} from '../api/client.js';
import { InstallLink, useInstallLink } from './InstallLink.js';
import { SlackLogo } from './SlackLogo.js';
import { useSlackReturn } from './slackReturn.js';
import { Workspaces } from './Workspaces.js';

// Slack's card on Settings' Connections: the DSP's Slack workspaces, which those who manage
// the DSP's connections add Dispatch to, by installing it themselves or by sending the install
// link to whoever runs their Slack.

export function SlackCard({ view }: { view: DspView }) {
  const account = useSlackAccount();
  const timeZone = view.dsp.timezone;
  const returned = useSlackReturn(async (state, code) => {
    await finishSlack(state, code);
    account.refresh();
  });
  // To Slack, to add a workspace, or with `team`, to reconnect or update one.
  const install = useAction(async (team?: string) => {
    const { url } = await connectSlack(team);
    window.location.assign(url);
  });
  const link = useInstallLink(account.refresh);
  const [leaving, setLeaving] = useState<SlackWorkspace>();
  const disconnect = useAction(
    async (team: string) => {
      await disconnectSlack(team);
      account.refresh();
      setLeaving(undefined);
    },
    { success: 'Slack disconnected' },
  );
  const busy = install.busy || returned.busy;
  return (
    <article className="archived-connection-card slack-connection-card">
      <header>
        <h3>
          <SlackLogo size={20} />
          Slack
        </h3>
      </header>
      <DataState data={account.data} error={account.error} retry={account.refresh}>
        {(data) =>
          !data.available ? (
            <div className="archived-connection-content">
              <div role="status">
                <Badge value="not_connected">Unavailable</Badge>
              </div>
            </div>
          ) : (
            <>
              <div className="archived-connection-content">
                {returned.busy ? (
                  <div role="status">
                    <Badge value="signing_in">Connecting</Badge>
                  </div>
                ) : (
                  !data.workspaces.length && (
                    <div role="status">
                      <Badge value="not_connected" />
                    </div>
                  )
                )}
                <ErrorBox message={returned.notice} />
                {data.workspaces.length > 0 && (
                  <Workspaces
                    workspaces={data.workspaces}
                    timeZone={timeZone}
                    busy={busy}
                    install={(team) => void install.run(team)}
                    disconnect={setLeaving}
                  />
                )}
                <InstallLink
                  link={data.link}
                  made={link.made}
                  timeZone={timeZone}
                  cancel={link.cancel}
                />
              </div>
              <footer>
                <button
                  className="primary slack-button"
                  disabled={busy}
                  onClick={() => void install.run()}
                >
                  {busy ? <LoaderCircle className="spin" size={16} /> : <SlackLogo size={16} />}
                  {data.workspaces.length ? 'Add workspace' : 'Connect Slack'}
                </button>
                <button
                  className="slack-button"
                  disabled={link.copy.busy}
                  onClick={() => void link.copy.run()}
                >
                  <Copy size={16} />
                  Copy install link
                </button>
              </footer>
            </>
          )
        }
      </DataState>
      {leaving && (
        <ConfirmDialog
          title={`Disconnect ${leaving.name}?`}
          confirm="Disconnect"
          tone="danger"
          busy={disconnect.busy}
          onConfirm={() => void disconnect.run(leaving.teamId)}
          onCancel={() => setLeaving(undefined)}
        >
          Dispatch is removed from {leaving.name} in Slack.
        </ConfirmDialog>
      )}
    </article>
  );
}
