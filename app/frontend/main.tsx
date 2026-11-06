import { lazy, Suspense, useState, useEffect, useLayoutEffect, useMemo } from 'react';
import { createRoot } from 'react-dom/client';
import type { SessionView } from '../../core/accounts/api/index.js';
import {
  FeedbackMessages,
  FeedbackProvider,
  useFeedback,
} from '../../core/shell/frontend/runtime/feedback.js';
import { askHolds, askToLeave } from '../../core/shell/frontend/runtime/holds.js';
import {
  followMoved,
  navigate,
  platformHash,
} from '../../core/shell/frontend/runtime/navigation.js';
import { Page, findRoute, navigation, sidebarOf } from './routes.js';
import { features } from './features.js';
import { routeLabel } from './route-meta.js';
import {
  dspSetup,
  installFeatures,
  loadReplyChecks,
} from '../../core/shell/frontend/runtime/slots.js';
import { useAppAdmission } from './useAppAdmission.js';
const loadAuth = () => import('../../core/accounts/frontend/index.js');
const SignedOutScreen = lazy(() =>
  loadAuth().then((module) => ({ default: module.SignedOutScreen })),
);
const DspOnboarding = lazy(() => loadAuth().then((module) => ({ default: module.DspOnboarding })));
const SecurityPrompt = lazy(() =>
  loadAuth().then((module) => ({ default: module.SecurityPrompt })),
);
import { Loading } from '../../core/shell/frontend/ui/Loading.js';
import { Modal } from '../../core/shell/frontend/ui/Modal.js';
import { PageBoundary } from '../../core/shell/frontend/ui/PageBoundary.js';
import { can } from '../../core/shell/frontend/runtime/permissions.js';
import '../../core/shell/frontend/styles.css';
import { Shell } from '../../core/shell/frontend/shell/Shell.js';
type Session = SessionView;
import { restoreAppearance } from '../../core/shell/frontend/runtime/appearance.js';
import { getSession } from '../../core/shell/frontend/runtime/endpoints.js';
import { loadSite } from '../../core/shell/frontend/runtime/site.js';
function App({ initialSession }: { initialSession: Promise<Session> }) {
  const {
    session,
    view,
    address,
    switching,
    sessionError,
    pending,
    showAuth,
    securityRequired,
    reauthenticate,
    load,
    reopen,
    logout,
    viewAs,
    closeVerification,
    completeVerification,
  } = useAppAdmission(initialSession, loadAuth);
  const [online, setOnline] = useState(navigator.onLine);
  const { perform } = useFeedback();
  useEffect(() => {
    const update = () => setOnline(navigator.onLine);
    window.addEventListener('online', update);
    window.addEventListener('offline', update);
    return () => {
      window.removeEventListener('online', update);
      window.removeEventListener('offline', update);
    };
  }, []);
  const setup = dspSetup();
  const setupRequired = Boolean(
    view?.profile?.setupRequired && setup && can(view, setup.permission),
  );
  const onboarding = address.route.startsWith('invite?') || (!showAuth && setupRequired);
  useLayoutEffect(() => {
    const apply = () => restoreAppearance(session?.user.id, onboarding ? 'light' : undefined);
    const media = matchMedia('(prefers-color-scheme: dark)');
    apply();
    media.addEventListener('change', apply);
    window.addEventListener('dispatch-appearance', apply);
    return () => {
      media.removeEventListener('change', apply);
      window.removeEventListener('dispatch-appearance', apply);
    };
  }, [session?.user.id, onboarding]);
  const { route, dspId, page } = address;
  // The app's sidebar, read anew only as the session, the DSP or its view change.
  const items = useMemo(
    () => (session ? navigation(dspId ? 'dsp' : 'platform', { session, view }) : []),
    [session, dspId, view],
  );
  if (session === undefined)
    return (
      <>
        {sessionError ? <button onClick={() => void load()}>Retry connection</button> : <Loading />}
        <FeedbackMessages />
      </>
    );
  if (showAuth || session === null)
    return <SignedOutScreen key={route} onLogin={() => load(true)} />;
  if (securityRequired && session)
    return (
      <SecurityPrompt security={session.security} complete={() => load(true)} signOut={logout} />
    );
  if (setupRequired && setup)
    return (
      <DspOnboarding
        saveTo={setup.save}
        code={view?.dsp.code}
        complete={() => load()}
        signOut={logout}
      />
    );
  const scope = dspId ? 'dsp' : 'platform';
  const opened = findRoute(scope, page);
  return (
    <Shell
      session={session}
      view={view}
      dspId={dspId}
      page={page}
      current={opened?.parent ?? page}
      label={routeLabel(scope, page)}
      pending={pending}
      // Only once the DSP's view is admitted: what shows before it, as Retry connection, doesn't fill.
      fills={Boolean(view) && opened?.scope === 'dsp' && opened.fills}
      navigation={items}
      sidebar={sidebarOf(scope, page, { session, view })}
      // Signing out asks about unsaved edits first, as leaving the page does.
      logout={() => askToLeave(() => void perform(logout))}
      exitView={() => navigate(platformHash())}
      // The page drawn anew for the role asks about unsaved edits first, as leaving it does.
      viewAs={viewAs}
    >
      <FeedbackMessages />
      {!online && (
        <p role="status">
          You’re offline. Showing the last loaded data; updates resume when you reconnect.
        </p>
      )}
      {reauthenticate && session && (
        <Modal title="Confirm it’s you" onClose={closeVerification} above>
          <Suspense fallback={<Loading />}>
            <SecurityPrompt
              security={session.security}
              complete={completeVerification}
              signOut={logout}
            />
          </Suspense>
        </Modal>
      )}
      {dspId ? (
        switching ? (
          <Loading />
        ) : view ? (
          <div key={view.token} className="dsp-page">
            <Page session={session} view={view} page={page} reopen={reopen} />
          </div>
        ) : (
          <button onClick={() => void perform(reopen)}>Retry connection</button>
        )
      ) : (
        <Page session={session} page={page} reopen={reopen} />
      )}
    </Shell>
  );
}
installFeatures(features);
// A page with unsaved edits asks before the address moves on, before any page follows it.
window.addEventListener('hashchange', askHolds);
const root = createRoot(document.getElementById('root')!);
// Which address this is decides what every link means here, so it is read first.
function start() {
  root.render(<Loading />);
  // Address and session are independent reads. Handle an early rejection while the
  // address is still loading; App consumes the same promise once it can open safely.
  const initialSession = getSession();
  void initialSession.catch(() => undefined);
  loadSite().then(
    () => {
      // A link to where a page once was opens it where it is now.
      followMoved();
      root.render(
        <FeedbackProvider>
          <PageBoundary>
            <Suspense fallback={<Loading />}>
              <App initialSession={initialSession} />
            </Suspense>
          </PageBoundary>
        </FeedbackProvider>,
      );
      // The owners' reply checks load beside the first page, so its replies rarely wait for
      // them. A failed load is tried again when a reply needs it.
      loadReplyChecks().catch(() => {});
    },
    () => root.render(<button onClick={start}>Retry connection</button>),
  );
}
start();
