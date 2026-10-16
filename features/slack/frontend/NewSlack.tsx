import { useState } from 'react';
import { ChevronRight, Copy, LoaderCircle } from 'lucide-react';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { Badge, DataState, Modal } from '../../../core/shell/frontend/ui/index.js';
import { connectSlack, useSlackAccount } from '../api/client.js';
import { useInstallLink } from './InstallLink.js';
import { SlackLogo } from './SlackLogo.js';

// Slack in New on Settings' DSP Connections: adding a workspace, with the name it's to have in
// Dispatch if one's given, by installing Dispatch there or by copying the install link for
// whoever runs it.

export function NewSlack({ close }: { close: () => void }) {
  const account = useSlackAccount();
  const [label, setLabel] = useState('');
  // To Slack, to add a workspace.
  const install = useAction(async () => {
    const { url } = await connectSlack({ label: label.trim() });
    window.location.assign(url);
  });
  const link = useInstallLink(close);
  const busy = install.busy || link.copy.busy;
  return (
    <Modal
      title={
        <span className="slack-new-title">
          <span className="slack-mark">
            <SlackLogo size={22} />
          </span>
          New Slack connection
        </span>
      }
      onClose={close}
      initialFocus="input"
    >
      <DataState data={account.data} error={account.error} retry={account.refresh}>
        {(data) =>
          !data.available ? (
            <div role="status">
              <Badge value="not_connected">Unavailable</Badge>
            </div>
          ) : link.made ? (
            <input
              className="slack-install-link"
              readOnly
              aria-label="Install link"
              value={link.made}
              onFocus={(event) => event.currentTarget.select()}
            />
          ) : (
            <div className="slack-new">
              <label>
                Name
                <input
                  value={label}
                  maxLength={60}
                  autoFocus
                  onChange={(event) => setLabel(event.target.value)}
                />
              </label>
              <div className="slack-ways">
                <button className="slack-way" disabled={busy} onClick={() => void install.run()}>
                  <span className="slack-way-top">
                    <span className="slack-way-icon">
                      {install.busy ? (
                        <LoaderCircle className="spin" size={18} />
                      ) : (
                        <SlackLogo size={18} />
                      )}
                    </span>
                    <ChevronRight size={16} />
                  </span>
                  Connect Slack
                </button>
                <button
                  className="slack-way"
                  disabled={busy}
                  onClick={() => void link.copy.run(label.trim())}
                >
                  <span className="slack-way-top">
                    <span className="slack-way-icon">
                      {link.copy.busy ? (
                        <LoaderCircle className="spin" size={18} />
                      ) : (
                        <Copy size={18} />
                      )}
                    </span>
                    <ChevronRight size={16} />
                  </span>
                  Copy install link
                </button>
              </div>
            </div>
          )
        }
      </DataState>
    </Modal>
  );
}
