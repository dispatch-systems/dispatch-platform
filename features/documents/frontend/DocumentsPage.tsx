import { useEffect, useRef, useState } from 'react';
import {
  ArrowRight,
  ExternalLink,
  FolderOpen,
  Info,
  LoaderCircle,
  ShieldCheck,
  TriangleAlert,
} from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { dspHash } from '../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { settingsPage } from '../../../core/shell/frontend/runtime/slots.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { DataState, ErrorBox } from '../../../core/shell/frontend/ui/index.js';
import {
  finishLink,
  linkGoogle,
  useDocumentsOverview,
  type DocumentsConnection,
  type DocumentsOverview,
  type MySharing,
} from '../api/client.js';
import { Browser } from './Browser.js';
import { GoogleLogo } from './GoogleLogo.js';
import { cancelled, isLink, settle, takeGoogleReturn } from './googleReturn.js';

// The DSP's Documents, once Google is connected and the member can edit in it: the folder is
// shared with them at a Google account. Its Google account is connected on Settings'
// Connections, so the page sends those who manage it there.

/** Where the DSP's Google account is connected: Settings' Connections. */
const connections = (view: DspView) => dspHash(view.dsp.id, settingsPage(), { tab: 'connections' });

export function DocumentsPage({ view }: { view: DspView }) {
  const overview = useDocumentsOverview();
  const manages = can(view, 'connections.manage');
  // A member who linked their own Google account from here comes back here.
  const [returned] = useState(() => {
    const found = takeGoogleReturn();
    return found && isLink(found) ? found : undefined;
  });
  const finishing = useAction(
    async (state: string, code: string) => {
      await finishLink(state, code);
      overview.refresh();
    },
    { success: 'Google account linked', inline: true },
  );
  const finished = useRef(false);
  useEffect(() => {
    if (!returned) return;
    settle(returned);
    if (!returned.code || finished.current) return;
    finished.current = true;
    void finishing.run(returned.state, returned.code);
  }, [returned, finishing]);
  const connection = overview.data?.connection;
  const me = overview.data?.me;
  // Until Documents first asks Google about the member, it does so now: ask again in a moment.
  const checking = connection?.status === 'connected' && me?.state === 'pending';
  useEffect(() => {
    if (!checking) return;
    const again = setTimeout(overview.refresh, 2000);
    return () => clearTimeout(again);
  }, [checking, overview.data, overview.refresh]);
  const notice = (returned?.error && cancelled(returned.error)) || finishing.error;
  // The platform owner isn't one of the DSP's members: the folder is never shared with them.
  if (connection?.status === 'connected' && !finishing.busy && (!me || me.state === 'shared'))
    return (
      <>
        <ErrorBox message={notice} />
        <Browser
          view={view}
          overview={overview.data!}
          connection={connection}
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
          {(data) => <Body view={view} data={data} manages={manages} />}
        </DataState>
      )}
    </>
  );
}

function Body({
  view,
  data,
  manages,
}: {
  view: DspView;
  data: DocumentsOverview;
  manages: boolean;
}) {
  const connection = data.connection;
  if (!connection) {
    if (!manages) return <NotSetUp />;
    return data.available ? <Connect view={view} /> : <Unavailable />;
  }
  if (connection.status === 'broken')
    return manages ? (
      <Broken view={view} connection={connection} />
    ) : (
      <BrokenForMember connection={connection} />
    );
  return data.me?.state === 'pending' ? <Checking /> : <NeedsGoogle me={data.me!} />;
}

function Connect({ view }: { view: DspView }) {
  return (
    <section className="documents-card documents-connect">
      <div className="documents-connect-copy">
        <h2>Connect Google to start using Documents</h2>
        <p>
          Your team's Google Drive account is connected in Settings, under DSP Connections. Dispatch
          makes a {view.dsp.name} folder in it and shares it with everyone who uses Documents.
          Anything your team creates here also shows up in their Google Drive.
        </p>
        <a className="documents-settings-button" href={connections(view)}>
          Go to Settings → Connections
          <ArrowRight size={16} />
        </a>
        <ol className="documents-steps">
          <li>In Settings → Connections, click Connect Google, sign in and click Allow.</li>
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

function Broken({ view, connection }: { view: DspView; connection: DocumentsConnection }) {
  return (
    <section className="documents-card documents-broken">
      <TriangleAlert size={22} />
      <div>
        <h2>Google stopped accepting Dispatch's connection</h2>
        <p>
          Your files are safe in Google Drive. Reconnect with{' '}
          <strong>{connection.accountEmail}</strong>, the account Documents was set up with, in
          Settings → Connections, so your team can open them here again.
        </p>
        <div className="documents-row">
          <a className="documents-settings-button" href={connections(view)}>
            Reconnect in Settings
            <ArrowRight size={16} />
          </a>
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

/** A moment after the member is given Use Documents, while Documents asks Google about them. */
function Checking() {
  return (
    <section className="documents-card documents-waiting" role="status">
      <span className="documents-folder-tile">
        <LoaderCircle className="spin" size={22} />
      </span>
      <h2>Getting your access ready…</h2>
      <p>Dispatch is sharing your team's folder with you. This takes a few seconds.</p>
    </section>
  );
}

/**
 * For a member Google won't share the folder with: their email isn't a Google account, or
 * Google refused for another reason. Linking a Google account opens Documents to them.
 */
function NeedsGoogle({ me }: { me: MySharing }) {
  const linking = useAction(async () => {
    const { url } = await linkGoogle();
    window.location.assign(url);
  });
  return (
    <section className="documents-card documents-waiting documents-gate">
      <span className="documents-folder-tile">
        <GoogleLogo size={24} />
      </span>
      {me.state === 'refused' ? (
        <>
          <h2>Google won't share your team's folder with you</h2>
          <p>
            Google refused to share it with <strong>{me.email}</strong>. Link a different Google
            account to open Documents, or ask your DSP owner.
          </p>
        </>
      ) : (
        <>
          <h2>Documents needs a Google account</h2>
          <p>
            Your team's files live in Google Drive, and <strong>{me.email}</strong> isn't a Google
            account. Link a Gmail or any Google account to open Documents. You'll still sign in to
            Dispatch with {me.email}.
          </p>
        </>
      )}
      <ErrorBox message={linking.error} />
      <button
        className="documents-google-button"
        disabled={linking.busy}
        onClick={() => void linking.run()}
      >
        <GoogleLogo />
        Link Google account
      </button>
      <p className="documents-gate-note">You can also do this anytime in Settings → Connections.</p>
    </section>
  );
}
