import { useEffect, useRef, useState } from 'react';
import { ExternalLink, FolderOpen, LoaderCircle, TriangleAlert } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { dspHash } from '../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import {
  Badge,
  ConfirmDialog,
  DataState,
  ErrorBox,
} from '../../../core/shell/frontend/ui/index.js';
import {
  disconnectGoogle,
  finishGoogle,
  finishLink,
  linkGoogle,
  useDocumentsAccount,
  useDocumentsOverview,
  type DocumentsConnection,
  type DriveStorage,
  type MySharing,
} from '../api/client.js';
import { GoogleLogo } from './GoogleLogo.js';
import { cancelled, isLink, settle, takeGoogleReturn, type GoogleReturn } from './googleReturn.js';
import { ReadyDialog } from './ReadyDialog.js';
import { useSignIn } from './signIn.js';

// Documents's cards on Settings' Connections: the DSP's Google account, which those who manage
// the DSP's connections connect, and each member's own Google account, which they edit in.

/**
 * Finishes the sign-in Google sent the browser back from, when it's this card's kind: once,
 * as soon as the card is drawn.
 */
function useReturn(
  mine: (returned: GoogleReturn) => boolean,
  finish: (state: string, code: string) => Promise<void>,
  success: string,
) {
  const [returned] = useState(() => {
    const found = takeGoogleReturn();
    return found && mine(found) ? found : undefined;
  });
  const finishing = useAction(finish, { success, inline: true });
  const started = useRef(false);
  useEffect(() => {
    if (!returned) return;
    settle(returned);
    if (!returned.code || started.current) return;
    started.current = true;
    void finishing.run(returned.state, returned.code);
  }, [returned, finishing]);
  return {
    busy: finishing.busy,
    notice: (returned?.error && cancelled(returned.error)) || finishing.error,
  };
}

const gigabytes = (bytes: number) => `${(bytes / 1_000_000_000).toFixed(1)} GB`;
/** How full the account is, with a warning once it is nearly full. */
function Storage({ storage }: { storage: DriveStorage }) {
  if (storage.limit === null) return <>{gigabytes(storage.used)} used</>;
  const share = Math.min(100, Math.round((storage.used / storage.limit) * 100));
  return (
    <div className="documents-storage">
      <span>
        {gigabytes(storage.used)} of {gigabytes(storage.limit)} used
      </span>
      <div className={`documents-meter${share >= 90 ? ' full' : ''}`} role="presentation">
        <span style={{ width: `${share}%` }} />
      </div>
      {share >= 90 && (
        <p className="documents-storage-warning">
          <TriangleAlert size={15} />
          Almost full. New files can't be added once it is. Add storage with Google One, or delete
          files you no longer need.
        </p>
      )}
    </div>
  );
}

const time = (at: string, timeZone: string) =>
  new Intl.DateTimeFormat('en-US', { timeZone, month: 'short', day: 'numeric' }).format(
    new Date(at),
  );

/** The DSP's Google account, which holds its Documents. */
export function GoogleDriveCard({ view }: { view: DspView }) {
  const account = useDocumentsAccount();
  const signIn = useSignIn();
  // Right after Dispatch makes the DSP's folder, folders to start with, for those who use it.
  const [ready, setReady] = useState<DocumentsConnection>();
  const returned = useReturn(
    (returned) => !isLink(returned),
    async (state, code) => {
      const connected = await finishGoogle(state, code);
      if (connected.madeFolder && can(view, 'documents.use'))
        setReady(connected.account.connection ?? undefined);
      account.refresh();
    },
    'Google connected',
  );
  const [disconnecting, setDisconnecting] = useState(false);
  const disconnect = useAction(
    async () => {
      await disconnectGoogle();
      account.refresh();
      setDisconnecting(false);
    },
    { success: 'Google disconnected' },
  );
  return (
    <>
      <article className="archived-connection-card documents-connection-card">
        <header>
          <h3>
            <GoogleLogo size={20} />
            Google Drive
          </h3>
        </header>
        <DataState data={account.data} error={account.error} retry={account.refresh}>
          {(data) => {
            const connection = data.connection;
            if (returned.busy)
              return (
                <div className="archived-connection-content">
                  <div role="status">
                    <Badge value="signing_in">Connecting</Badge>
                  </div>
                  <p className="muted documents-row">
                    <LoaderCircle className="spin" size={16} /> Finishing with Google…
                  </p>
                </div>
              );
            if (!connection)
              return (
                <>
                  <div className="archived-connection-content">
                    <div role="status">
                      <Badge value="not_connected" />
                    </div>
                    <ErrorBox message={returned.notice} />
                    <p className="muted">
                      {data.available
                        ? "Connect a Google account to keep your team's Documents in its Google Drive. Use your company's Google Workspace, or a Gmail made just for the business."
                        : "Google isn't set up for Dispatch yet. Dispatch support needs to finish setting it up."}
                    </p>
                  </div>
                  {data.available && (
                    <footer>
                      <button
                        className="primary documents-row"
                        disabled={signIn.busy}
                        onClick={() => void signIn.run()}
                      >
                        <GoogleLogo size={16} />
                        Connect Google
                      </button>
                    </footer>
                  )}
                </>
              );
            const broken = connection.status === 'broken';
            return (
              <>
                <div className="archived-connection-content">
                  <div role="status">
                    {broken ? (
                      <Badge value="failed">Needs Reconnecting</Badge>
                    ) : (
                      <Badge value="ready" />
                    )}
                  </div>
                  <ErrorBox message={returned.notice} />
                  {broken && (
                    <p className="muted">
                      Google stopped accepting Dispatch's access to{' '}
                      <strong>{connection.accountEmail}</strong>, so your team can't open Documents.
                      Your files are safe in Google Drive. Reconnect with that same account to pick
                      up where you left off.
                    </p>
                  )}
                  <dl className="documents-details">
                    <dt>Account</dt>
                    <dd>
                      {connection.accountEmail}
                      <small>
                        {connection.accountKind === 'workspace'
                          ? 'Google Workspace'
                          : 'Personal Google account'}
                      </small>
                    </dd>
                    <dt>Folder</dt>
                    <dd>{connection.folderName}</dd>
                    {data.storage && (
                      <>
                        <dt>Storage</dt>
                        <dd>
                          <Storage storage={data.storage} />
                        </dd>
                      </>
                    )}
                    <dt>Connected</dt>
                    <dd>
                      {time(connection.connectedAt, view.dsp.timezone)}
                      {connection.connectedBy ? `, by ${connection.connectedBy}` : ''}
                    </dd>
                  </dl>
                </div>
                <footer>
                  {broken && (
                    <button
                      className="primary documents-row"
                      disabled={signIn.busy}
                      onClick={() => void signIn.run()}
                    >
                      <GoogleLogo size={16} />
                      Reconnect Google
                    </button>
                  )}
                  <a
                    className="documents-link-button"
                    href={connection.folderUrl}
                    target="_blank"
                    rel="noreferrer"
                  >
                    <ExternalLink size={16} />
                    Open in Drive
                  </a>
                  <button className="text-button" onClick={() => setDisconnecting(true)}>
                    Disconnect
                  </button>
                </footer>
                {disconnecting && (
                  <ConfirmDialog
                    title="Disconnect Google?"
                    confirm="Disconnect"
                    tone="danger"
                    busy={disconnect.busy}
                    onConfirm={() => void disconnect.run()}
                    onCancel={() => setDisconnecting(false)}
                  >
                    Your team stops seeing Documents in Dispatch, and Dispatch's access to{' '}
                    {connection.accountEmail} is removed. {connection.folderName} and everything in
                    it stay in that account's Google Drive, shared as it is.
                  </ConfirmDialog>
                )}
              </>
            );
          }}
        </DataState>
      </article>
      {ready && (
        <ReadyDialog
          connection={ready}
          close={() => setReady(undefined)}
          made={() => setReady(undefined)}
        />
      )}
    </>
  );
}

/** What the member's own Google account card says, by how the folder is shared with them. */
function said(me: MySharing, signInEmail: string) {
  if (me.state === 'pending')
    return {
      badge: <Badge value="signing_in">Checking</Badge>,
      text: (
        <>
          Checking with Google whether <strong>{me.email}</strong> is a Google account. This takes a
          few seconds.
        </>
      ),
    };
  if (me.state === 'shared')
    return {
      badge: <Badge value="ready" />,
      text: me.linked ? (
        <>
          Linked to <strong>{me.email}</strong>, so you can open and edit your team's Documents in
          Google. You still sign in to Dispatch with {signInEmail}.
        </>
      ) : (
        <>
          <strong>{me.email}</strong> is the email you sign in with, and it's a Google account, so
          you can open and edit your team's Documents in Google.
        </>
      ),
    };
  if (me.state === 'refused')
    return {
      badge: <Badge value="failed">Not Shared</Badge>,
      text: (
        <>
          Google won't share your team's folder with <strong>{me.email}</strong>. Link a different
          Google account, or ask your DSP owner.
        </>
      ),
    };
  return {
    badge: <Badge value="not_connected" />,
    text: (
      <>
        <strong>{me.email}</strong> isn't a Google account, so you can't open Documents yet. Link a
        Gmail or any Google account. You'll still sign in to Dispatch with {signInEmail}.
      </>
    ),
  };
}

/** The member's own Google account, which they open and edit their team's Documents with. */
export function GoogleAccountCard({ view, email }: { view: DspView; email: string }) {
  const overview = useDocumentsOverview();
  const returned = useReturn(
    isLink,
    async (state, code) => {
      await finishLink(state, code);
      overview.refresh();
    },
    'Google account linked',
  );
  const linking = useAction(async () => {
    const { url } = await linkGoogle('settings');
    window.location.assign(url);
  });
  const me = overview.data?.me;
  // Until Documents first asks Google, it does so now; the card asks again in a moment.
  useEffect(() => {
    if (me?.state !== 'pending') return;
    const again = setTimeout(overview.refresh, 3000);
    return () => clearTimeout(again);
  }, [me?.state, overview.refresh]);
  return (
    <article className="archived-connection-card documents-connection-card">
      <header>
        <h3>
          <GoogleLogo size={20} />
          Google account
        </h3>
      </header>
      <DataState data={overview.data} error={overview.error} retry={overview.refresh}>
        {(data) => {
          if (!data.me) return null;
          if (!data.connection)
            return (
              <div className="archived-connection-content">
                <div role="status">
                  <Badge value="idle">Waiting</Badge>
                </div>
                <p className="muted">
                  Your DSP hasn't connected its Google Drive yet. Once it has, this says whether you
                  can edit your team's Documents in Google.
                </p>
              </div>
            );
          if (returned.busy)
            return (
              <div className="archived-connection-content">
                <p className="muted documents-row">
                  <LoaderCircle className="spin" size={16} /> Finishing with Google…
                </p>
              </div>
            );
          const { badge, text } = said(data.me, email);
          const linkable = data.me.state !== 'pending';
          return (
            <>
              <div className="archived-connection-content">
                <div role="status">{badge}</div>
                <ErrorBox message={returned.notice || linking.error} />
                <p className="muted">{text}</p>
              </div>
              {linkable && (
                <footer>
                  {data.me.state === 'shared' ? (
                    <button
                      className="text-button"
                      disabled={linking.busy}
                      onClick={() => void linking.run()}
                    >
                      Use a different Google account
                    </button>
                  ) : (
                    <button
                      className="primary documents-row"
                      disabled={linking.busy}
                      onClick={() => void linking.run()}
                    >
                      <GoogleLogo size={16} />
                      Link Google account
                    </button>
                  )}
                  {data.me.state === 'shared' && (
                    <a className="documents-link-button" href={dspHash(view.dsp.id, 'documents')}>
                      <FolderOpen size={16} />
                      Open Documents
                    </a>
                  )}
                </footer>
              )}
            </>
          );
        }}
      </DataState>
    </article>
  );
}
