import { useState } from 'react';
import { Copy } from 'lucide-react';
import type { CommunicationsWorkspace } from '../../api/index.js';
import { connectSlack, createWorkspaceLink } from '../../../slack/api/client.js';
import { SlackLogo } from '../../../slack/frontend/index.js';
import { Modal } from '../../../../core/shell/frontend/ui/index.js';
import { toClipboard } from '../../../../core/shell/frontend/runtime/clipboard.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { WorkspaceMark } from './WorkspaceMark.js';
import './setup.css';

// Setting Slack up from Communications, for those who manage the DSP's connections: connecting
// it, in place of the inbox, while the DSP has no workspace; and updating each workspace that
// hasn't approved what Communications uses, over the page while it shows Slack, until it's done.
// Either comes back here from Slack.

/** This page, where Slack sends the browser back to finish. */
const PAGE = 'communications';

/** To Slack, to add a workspace, or update the workspace `team`. */
async function install(asked: { team: string } | { label: string }) {
  const { url } = await connectSlack({ ...asked, page: PAGE });
  window.location.assign(url);
}

export function ConnectSlack({ busy }: { busy: boolean }) {
  const connect = useAction(() => install({ label: '' }));
  return (
    <div className="setup">
      <span className="setup-mark">
        <SlackLogo size={38} />
      </span>
      <div className="setup-why">
        <h2>Employees message your DSP through Slack</h2>
        <p>Connect Slack to see and answer their messages here.</p>
      </div>
      <button
        className="primary"
        disabled={busy || connect.busy}
        onClick={() => void connect.run()}
      >
        Connect Slack
      </button>
    </div>
  );
}

/**
 * Over the page alone while a workspace needs updating, with no way to close it but to another
 * section the page has (`close`): each one updated here, or its install link copied for whoever
 * can approve apps in that Slack. It goes once the page reads that none does.
 */
export function UpdateSlack({
  workspaces,
  busy,
  close,
}: {
  workspaces: readonly CommunicationsWorkspace[];
  busy: boolean;
  /** Leaves it for another section, while there's one. */
  close?: () => void;
}) {
  return (
    <Modal
      page
      dismissible={Boolean(close)}
      onClose={close ?? (() => undefined)}
      title={
        <span className="update-title">
          <span className="update-mark">
            <SlackLogo size={20} />
          </span>
          Update Slack
        </span>
      }
    >
      <p className="update-why">Slack needs new permissions so employees can message your DSP.</p>
      <ul className="update-workspaces">
        {workspaces.map((workspace) => (
          <UpdateRow key={workspace.teamId} workspace={workspace} busy={busy} />
        ))}
      </ul>
    </Modal>
  );
}

function UpdateRow({ workspace, busy }: { workspace: CommunicationsWorkspace; busy: boolean }) {
  const { notify } = useFeedback();
  // The link made here, for a browser that kept it from the clipboard: shown this once, as
  // Dispatch keeps only its hash.
  const [made, setMade] = useState('');
  const copy = useAction(async () => {
    const link = createWorkspaceLink(workspace.teamId).then((created) => created.url);
    const copied = await toClipboard(link).then(
      () => true,
      () => false,
    );
    const url = await link;
    if (copied) notify('Install link copied');
    else setMade(url);
  });
  const update = useAction(() => install({ team: workspace.teamId }));
  return (
    <li className="update-workspace">
      <WorkspaceMark workspace={workspace} />
      <strong>{workspace.name}</strong>
      <span className="update-buttons">
        {made ? (
          <input
            readOnly
            aria-label="Install link"
            value={made}
            onFocus={(event) => event.currentTarget.select()}
          />
        ) : (
          <button disabled={copy.busy} onClick={() => void copy.run()}>
            <Copy size={15} />
            Copy link
          </button>
        )}
        <button
          className="primary"
          disabled={busy || update.busy}
          onClick={() => void update.run()}
        >
          Update
        </button>
      </span>
    </li>
  );
}
