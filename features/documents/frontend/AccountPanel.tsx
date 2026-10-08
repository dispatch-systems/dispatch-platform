import { useState } from 'react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { time } from '../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { ConfirmDialog, Modal } from '../../../core/shell/frontend/ui/index.js';
import { disconnectGoogle, type DocumentsConnection } from '../api/client.js';
import { GoogleLogo } from './GoogleLogo.js';
import { useSignIn } from './signIn.js';

/** The Google account that holds the DSP's Documents, for those who manage it. */
export function AccountPanel({
  view,
  connection,
  close,
  disconnected,
}: {
  view: DspView;
  connection: DocumentsConnection;
  close: () => void;
  disconnected: () => void;
}) {
  const signIn = useSignIn();
  const [confirming, setConfirming] = useState(false);
  const disconnect = useAction(
    async () => {
      await disconnectGoogle();
      setConfirming(false);
      close();
      disconnected();
    },
    { success: 'Google disconnected' },
  );
  const holder =
    connection.accountKind === 'workspace' ? 'Google Workspace account' : 'Google account';
  return (
    <Modal variant="sheet" title="Google account" onClose={close}>
      <div className="panel-body documents-account">
        <p className="muted">
          Everyone at {view.dsp.name} works in this account's {connection.folderName} folder through
          Dispatch.
        </p>
        <section className="documents-account-card">
          <GoogleLogo size={22} />
          <div>
            <strong>{connection.accountEmail}</strong>
            <small>
              {holder} that holds the {connection.folderName} folder
            </small>
          </div>
          <button disabled={signIn.busy} onClick={() => void signIn.run()}>
            Reconnect
          </button>
        </section>
        <footer className="documents-account-foot">
          <span className="muted">
            Connected {time(connection.connectedAt, view.dsp.timezone)}
            {connection.connectedBy ? ` by ${connection.connectedBy}` : ''}
          </span>
          <button className="documents-disconnect" onClick={() => setConfirming(true)}>
            Disconnect Google
          </button>
        </footer>
      </div>
      {confirming && (
        <ConfirmDialog
          title="Disconnect Google?"
          confirm="Disconnect"
          tone="danger"
          busy={disconnect.busy}
          onConfirm={() => void disconnect.run()}
          onCancel={() => setConfirming(false)}
        >
          Your team stops seeing Documents here, and Dispatch's access to {connection.accountEmail}{' '}
          is removed. {connection.folderName} and everything in it stay in that account's Google
          Drive.
        </ConfirmDialog>
      )}
    </Modal>
  );
}
