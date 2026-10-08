import { useEffect, useRef, useState } from 'react';
import {
  ExternalLink,
  FolderOpen,
  Info,
  LoaderCircle,
  ShieldCheck,
  TriangleAlert,
} from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { time } from '../../../core/shell/frontend/lib/format.js';
import { hashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { ConfirmDialog, DataState, ErrorBox } from '../../../core/shell/frontend/ui/index.js';
import {
  connectGoogle,
  disconnectGoogle,
  finishGoogle,
  useDocumentsOverview,
  type DocumentsConnection,
  type DocumentsOverview,
} from '../api/client.js';
import { GoogleLogo } from './GoogleLogo.js';

/**
 * What Google sent the browser back with, read once and taken out of the address, so the
 * code is neither used twice nor left in the browser's history.
 */
function takeGoogleReturn() {
  const query = hashQuery();
  const state = query.get('googleState');
  if (!state) return undefined;
  const page = location.hash.split('?')[0];
  history.replaceState(history.state, '', `${location.pathname}${location.search}${page}`);
  return { state, code: query.get('googleCode') ?? '', error: query.get('googleError') ?? '' };
}
const cancelled = (error: string) =>
  error === 'access_denied'
    ? 'Google sign-in was cancelled, so nothing changed.'
    : "Google didn't finish signing in. Try connecting again.";

/** Whom to ask, at the start of a sentence: "Maria Flores", or any owner when there are more. */
const askOf = (owners: string[]) => (owners.length === 1 ? owners[0] : 'An owner of your DSP');

export function DocumentsPage({ view }: { view: DspView }) {
  const overview = useDocumentsOverview();
  const canManage = can(view, 'documents.manage');
  const [returned] = useState(takeGoogleReturn);
  const finishing = useAction(
    async (state: string, code: string) => {
      await finishGoogle(state, code);
      overview.refresh();
    },
    { success: 'Google connected', inline: true },
  );
  const finished = useRef(false);
  useEffect(() => {
    if (!returned?.code || finished.current) return;
    finished.current = true;
    void finishing.run(returned.state, returned.code);
  }, [returned, finishing]);
  const connection = overview.data?.connection;
  return (
    <>
      <div className="page-heading">
        <div>
          <h1>Documents</h1>
          <p>
            {connection
              ? `Shared with everyone at ${view.dsp.name}. Anything added here also shows up in Google Drive.`
              : "Your team's folders, Docs and Sheets, kept in your Google Drive."}
          </p>
        </div>
        {connection && (
          <div className="heading-actions">
            <a
              className="documents-link-button"
              href={connection.folderUrl}
              target="_blank"
              rel="noreferrer"
            >
              <ExternalLink size={16} />
              Open in Drive
            </a>
          </div>
        )}
      </div>
      <ErrorBox message={(returned?.error && cancelled(returned.error)) || finishing.error} />
      {finishing.busy ? (
        <div className="loading" role="status">
          <LoaderCircle className="spin" size={20} /> Finishing with Google…
        </div>
      ) : (
        <DataState data={overview.data} error={overview.error} retry={overview.refresh}>
          {(data) => (
            <Body view={view} data={data} canManage={canManage} refresh={overview.refresh} />
          )}
        </DataState>
      )}
    </>
  );
}

function Body({
  view,
  data,
  canManage,
  refresh,
}: {
  view: DspView;
  data: DocumentsOverview;
  canManage: boolean;
  refresh: () => void;
}) {
  const connection = data.connection;
  if (!connection) {
    if (!canManage) return <NotSetUp owners={data.owners} />;
    return data.available ? <Connect dspName={view.dsp.name} /> : <Unavailable />;
  }
  if (connection.status === 'broken')
    return canManage ? (
      <Broken connection={connection} />
    ) : (
      <BrokenForMember connection={connection} owners={data.owners} />
    );
  return <Connected view={view} connection={connection} canManage={canManage} refresh={refresh} />;
}

function useSignIn() {
  return useAction(async () => {
    const { url } = await connectGoogle();
    window.location.assign(url);
  });
}

function Connect({ dspName }: { dspName: string }) {
  const signIn = useSignIn();
  return (
    <section className="documents-card documents-connect">
      <div className="documents-connect-copy">
        <h2>Connect Google to start using Documents</h2>
        <p>
          Dispatch makes a {dspName} folder in the Google account you connect and shares it with
          your team. Anything your team creates here also shows up in their Google Drive.
        </p>
        <button
          className="documents-google-button"
          disabled={signIn.busy}
          onClick={() => void signIn.run()}
        >
          <GoogleLogo />
          Connect Google
        </button>
        <ol className="documents-steps">
          <li>Sign in with Google and click Allow.</li>
          <li>
            Dispatch makes your team's folder.
            <small>About 10 seconds.</small>
          </li>
          <li>
            Everyone on your team gets access.
            <small>People who join later get it too, and people who leave lose it.</small>
          </li>
        </ol>
        <div className="documents-fine">
          <p>
            <ShieldCheck size={16} />
            Dispatch can only open the files it makes or that someone adds to it. It can't see
            anything else in your Drive.
          </p>
          <p>
            <Info size={16} />
            Use a Google account the business keeps: your company Google Workspace account, or a
            free Gmail made just for the business. Files made here belong to it.
          </p>
          <p>
            <Info size={16} />
            Turn on 2-step verification for that account. A free Gmail has 15 GB of storage, shared
            with its email and photos.
          </p>
        </div>
      </div>
    </section>
  );
}

function Unavailable() {
  return (
    <section className="documents-card documents-waiting">
      <h2>Google isn't set up for Dispatch yet</h2>
      <p>Dispatch support needs to finish setting up Google before any DSP can connect it.</p>
    </section>
  );
}

function NotSetUp({ owners }: { owners: string[] }) {
  return (
    <section className="documents-card documents-waiting">
      <span className="documents-folder-tile">
        <FolderOpen size={22} />
      </span>
      <h2>Documents isn't set up yet</h2>
      <p>
        {askOf(owners)} needs to connect your company's Google account. Your team's files will show
        up here once that's done.
      </p>
    </section>
  );
}

function Broken({ connection }: { connection: DocumentsConnection }) {
  const signIn = useSignIn();
  return (
    <section className="documents-card documents-broken">
      <TriangleAlert size={22} />
      <div>
        <h2>Google stopped accepting Dispatch's connection</h2>
        <p>
          Your files are safe in Google Drive. Reconnect with{' '}
          <strong>{connection.accountEmail}</strong>, the account Documents was set up with, so your
          team can open them here again.
        </p>
        <div className="documents-row">
          <button
            className="documents-google-button"
            disabled={signIn.busy}
            onClick={() => void signIn.run()}
          >
            <GoogleLogo />
            Reconnect Google
          </button>
          <a
            className="documents-link-button"
            href={connection.folderUrl}
            target="_blank"
            rel="noreferrer"
          >
            <ExternalLink size={16} />
            Open Google Drive
          </a>
        </div>
      </div>
    </section>
  );
}

function BrokenForMember({
  connection,
  owners,
}: {
  connection: DocumentsConnection;
  owners: string[];
}) {
  return (
    <section className="documents-card documents-broken">
      <TriangleAlert size={22} />
      <div>
        <h2>Documents can't reach Google right now</h2>
        <p>
          Your team's files are safe in {connection.accountEmail}'s Google Drive. {askOf(owners)}{' '}
          needs to reconnect Google.
        </p>
      </div>
    </section>
  );
}

function Connected({
  view,
  connection,
  canManage,
  refresh,
}: {
  view: DspView;
  connection: DocumentsConnection;
  canManage: boolean;
  refresh: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const disconnect = useAction(
    async () => {
      await disconnectGoogle();
      setConfirming(false);
      refresh();
    },
    { success: 'Google disconnected' },
  );
  return (
    <section className="documents-card documents-connected">
      <span className="documents-folder-tile">
        <FolderOpen size={22} />
      </span>
      <div>
        <h2>{connection.folderName}</h2>
        <p>
          In {connection.accountEmail}'s Google Drive. Connected{' '}
          {connection.connectedBy ? `by ${connection.connectedBy} ` : ''}
          on {time(connection.connectedAt, view.dsp.timezone)}.
        </p>
      </div>
      {canManage && (
        <button className="documents-disconnect" onClick={() => setConfirming(true)}>
          Disconnect Google
        </button>
      )}
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
    </section>
  );
}
