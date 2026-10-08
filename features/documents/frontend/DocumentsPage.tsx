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
import { hashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { DataState, ErrorBox } from '../../../core/shell/frontend/ui/index.js';
import {
  finishGoogle,
  finishLink,
  useDocumentsOverview,
  type DocumentsConnection,
  type DocumentsOverview,
} from '../api/client.js';
import { Browser } from './Browser.js';
import { GoogleLogo } from './GoogleLogo.js';
import { useSignIn } from './signIn.js';

type GoogleReturn = { state: string; code: string; error: string };
/** What Google sent back, until the page that took it starts finishing it. */
let pending: GoogleReturn | undefined;
/**
 * What Google sent the browser back with, taken out of the address so the code is neither
 * used twice nor left in the browser's history. It is kept until the page starts finishing
 * it: React may set aside a page's first render and render it again, and that render finds
 * the address already clean.
 */
function takeGoogleReturn() {
  const query = hashQuery();
  const state = query.get('googleState');
  if (state) {
    const page = location.hash.split('?')[0];
    history.replaceState(history.state, '', `${location.pathname}${location.search}${page}`);
    pending = { state, code: query.get('googleCode') ?? '', error: query.get('googleError') ?? '' };
  }
  return pending;
}
const cancelled = (error: string) =>
  error === 'access_denied'
    ? 'Google sign-in was cancelled, so nothing changed.'
    : "Google didn't finish signing in. Try connecting again.";

export function DocumentsPage({ view }: { view: DspView }) {
  const overview = useDocumentsOverview();
  const canManage = can(view, 'documents.manage');
  const [returned] = useState(takeGoogleReturn);
  // Whether Google was connected on this visit, so Documents offers folders to start with.
  const [connected, setConnected] = useState(false);
  // A member linking their own Google account comes back here too: its sign-in says so.
  const linking = Boolean(returned?.state.includes('.link.'));
  const finishing = useAction(
    async (state: string, code: string) => {
      if (linking) await finishLink(state, code);
      else {
        await finishGoogle(state, code);
        setConnected(true);
      }
      overview.refresh();
    },
    { success: linking ? 'Google account linked' : 'Google connected', inline: true },
  );
  const finished = useRef(false);
  useEffect(() => {
    // The page shown took it: a later visit to Documents doesn't find it again.
    if (returned === pending) pending = undefined;
    if (!returned?.code || finished.current) return;
    finished.current = true;
    void finishing.run(returned.state, returned.code);
  }, [returned, finishing]);
  const connection = overview.data?.connection;
  const notice = (returned?.error && cancelled(returned.error)) || finishing.error;
  if (connection?.status === 'connected' && !finishing.busy)
    return (
      <>
        <ErrorBox message={notice} />
        <Browser
          view={view}
          overview={overview.data!}
          connection={connection}
          canManage={canManage}
          connected={connected}
          overviewChanged={overview.refresh}
        />
      </>
    );
  return (
    <>
      <div className="page-heading">
        <div>
          <h1>Documents</h1>
          <p>Your team's folders, Docs and Sheets, kept in your Google Drive.</p>
        </div>
      </div>
      <ErrorBox message={notice} />
      {finishing.busy ? (
        <div className="loading" role="status">
          <LoaderCircle className="spin" size={20} /> Finishing with Google…
        </div>
      ) : (
        <DataState data={overview.data} error={overview.error} retry={overview.refresh}>
          {(data) => <Body view={view} data={data} canManage={canManage} />}
        </DataState>
      )}
    </>
  );
}

function Body({
  view,
  data,
  canManage,
}: {
  view: DspView;
  data: DocumentsOverview;
  canManage: boolean;
}) {
  const connection = data.connection;
  if (!connection) {
    if (!canManage) return <NotSetUp />;
    return data.available ? <Connect dspName={view.dsp.name} /> : <Unavailable />;
  }
  return canManage ? (
    <Broken connection={connection} />
  ) : (
    <BrokenForMember connection={connection} />
  );
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

function NotSetUp() {
  return (
    <section className="documents-card documents-waiting">
      <span className="documents-folder-tile">
        <FolderOpen size={22} />
      </span>
      <h2>Documents isn't set up yet</h2>
      <p>
        Your DSP owner needs to set it up. Your team's files will show up here once that's done.
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

function BrokenForMember({ connection }: { connection: DocumentsConnection }) {
  return (
    <section className="documents-card documents-broken">
      <TriangleAlert size={22} />
      <div>
        <h2>Documents can't reach Google right now</h2>
        <p>
          Your team's files are safe in {connection.accountEmail}'s Google Drive. Your DSP owner
          needs to reconnect Google.
        </p>
      </div>
    </section>
  );
}
