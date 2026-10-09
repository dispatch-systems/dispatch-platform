import {
  useBrowserUpdate,
  clearNavigationState,
} from '../../core/shell/frontend/runtime/browser-update.js';
import {
  lazy,
  Suspense,
  useState,
  useEffect,
  useLayoutEffect,
  useCallback,
  useRef,
  useTransition,
} from 'react';
import { createRoot } from 'react-dom/client';
import { flushSync } from 'react-dom';
import type { DspView, SessionView } from '../../core/accounts/api/index.js';
import {
  api,
  credentials,
  ApiError,
  csrf as activeCsrf,
  view as admittedToken,
} from '../../core/shell/frontend/runtime/api.js';
import {
  FeedbackMessages,
  FeedbackProvider,
  useFeedback,
} from '../../core/shell/frontend/runtime/feedback.js';
import {
  dspHash,
  forgetDestination,
  navigate,
  parseHash,
  platformHash,
  rememberDestination,
} from '../../core/shell/frontend/runtime/navigation.js';
import {
  Page,
  canNavigateImmediately,
  findRoute,
  landing,
  navigation,
  prepareRoute,
} from './routes.js';
import { features } from './features.js';
import { routeLabel } from './route-meta.js';
import {
  dspSetup,
  installFeatures,
  landingPage,
  type DspRouteId,
} from '../../core/shell/frontend/runtime/slots.js';
const loadAuth = () => import('../../core/accounts/frontend/index.js');
const SignedOutScreen = lazy(() =>
  loadAuth().then((module) => ({ default: module.SignedOutScreen })),
);
const DspOnboarding = lazy(() => loadAuth().then((module) => ({ default: module.DspOnboarding })));
const SecurityPrompt = lazy(() =>
  loadAuth().then((module) => ({ default: module.SecurityPrompt })),
);
import { messageOf } from '../../core/shell/frontend/lib/errors.js';
import { Loading } from '../../core/shell/frontend/ui/Loading.js';
import { Modal } from '../../core/shell/frontend/ui/Modal.js';
import { PageBoundary } from '../../core/shell/frontend/ui/PageBoundary.js';
import { can } from '../../core/shell/frontend/runtime/permissions.js';
import { hasFeature } from '../../core/shell/frontend/runtime/features.js';
import '../../core/shell/frontend/styles.css';
import { Shell } from '../../core/shell/frontend/shell/Shell.js';
type Session = SessionView;
import { restoreAppearance } from '../../core/shell/frontend/runtime/appearance.js';
import { leavePresence, usePresence } from '../../core/shell/frontend/runtime/presence.js';
import { openView, saveRole } from '../../core/shell/frontend/runtime/session.js';
import { getSession } from '../../core/shell/frontend/runtime/endpoints.js';
import { loadSite } from '../../core/shell/frontend/runtime/site.js';
import { cancelPrefetches } from '../../core/shell/frontend/runtime/prefetch.js';
function App() {
  const [pending, startTransition] = useTransition();
  const admission = useRef(0);
  const pendingAdmission = useRef<number>(undefined);
  const sessionRequest = useRef(0);
  const [reauthenticate, setReauthenticate] = useState(false);
  const [session, setSession] = useState<Session | null>(),
    [view, setView] = useState<DspView>(),
    [address, setAddress] = useState(() => parseHash(window.location.hash)),
    [switching, setSwitching] = useState(false),
    [sessionError, setSessionError] = useState(''),
    [online, setOnline] = useState(navigator.onLine);
  const { perform, fail } = useFeedback();
  useEffect(() => {
    const update = () => setOnline(navigator.onLine);
    window.addEventListener('online', update);
    window.addEventListener('offline', update);
    return () => {
      window.removeEventListener('online', update);
      window.removeEventListener('offline', update);
    };
  }, []);
  const securityRequired = Boolean(session?.security.required && !session.security.verified);
  const setup = dspSetup();
  const setupRequired = Boolean(
    view?.profile?.setupRequired && setup && can(view, setup.permission),
  );
  const showAuth =
    session === null ||
    address.route === 'signin' ||
    address.route.startsWith('invite?') ||
    address.route.startsWith('reset?');
  const onboarding = address.route.startsWith('invite?') || (!showAuth && setupRequired);
  const navigationState = useRef({ address, session, view, showAuth, securityRequired });
  useLayoutEffect(() => {
    navigationState.current = { address, session, view, showAuth, securityRequired };
  }, [address, session, view, showAuth, securityRequired]);
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
  const load = useCallback(
    async (afterLogin = false) => {
      const request = ++sessionRequest.current;
      admission.current++;
      cancelPrefetches();
      credentials(activeCsrf);
      setView(undefined);
      setSwitching(
        Boolean(parseHash(window.location.hash).dspId && navigationState.current.session),
      );
      setSessionError('');
      try {
        const next = await getSession();
        if (request !== sessionRequest.current) return;
        credentials(next.csrf);
        setView(undefined);
        setSession(next);
        if (
          !next.user.platformOwner &&
          (afterLogin || !/^#(?:invite\?|reset\?|signin)/.test(window.location.hash)) &&
          !parseHash(window.location.hash).dspId &&
          next.dsps.length === 1
        )
          navigate(dspHash(next.dsps[0]!.id));
      } catch (error) {
        if (request !== sessionRequest.current) return;
        setSwitching(false);
        if (error instanceof ApiError && error.status === 401) {
          credentials('');
          setSession(null);
        } else {
          setSessionError(messageOf(error));
          fail(messageOf(error));
        }
      }
    },
    [fail],
  );
  useEffect(() => {
    void load();
    const initial = parseHash(window.location.hash);
    // Session and code are independent. In particular, sign-in should not wait for
    // the session's unauthenticated response before downloading its form.
    if (!initial.dspId) void loadAuth().catch(() => undefined);
    else void prepareRoute('dsp', initial.page).catch(() => undefined);
  }, [load]);
  useEffect(() => {
    const changed = () => {
      const next = parseHash(window.location.hash);
      const current = navigationState.current;
      const auth = /^(?:signin$|invite\?|reset\?)/.test(next.route);
      const sameWorkspace =
        Boolean(current.session) &&
        !current.showAuth &&
        !current.securityRequired &&
        !auth &&
        current.address.dspId === next.dspId;
      const sameScope =
        sameWorkspace &&
        (!next.dspId ||
          (current.view?.dsp.id === next.dspId && current.view.token === admittedToken));
      cancelPrefetches();
      if (sameScope && current.session) {
        const scope = next.dspId ? 'dsp' : 'platform';
        const access = {
          session: current.session,
          view: next.dspId ? current.view : undefined,
        };
        const ready = canNavigateImmediately(scope, next.page, access);
        void prepareRoute(scope, next.page, access, true).catch(() => undefined);
        if (ready) flushSync(() => setAddress(next));
        else startTransition(() => setAddress(next));
      } else {
        // A workspace/security boundary must discard the previous page immediately.
        // Changing pages while this same workspace is opening keeps its admission;
        // its eventual response opens the latest address rather than leaving a spinner.
        const opening =
          sameWorkspace &&
          next.dspId &&
          !admittedToken &&
          pendingAdmission.current === admission.current;
        if (opening) void prepareRoute('dsp', next.page).catch(() => undefined);
        else admission.current++;
        // A lazy security screen may not have committed the newly accepted session yet.
        // Dropping the DSP view must preserve that session's live CSRF credential.
        credentials(activeCsrf);
        setView(undefined);
        setSwitching(Boolean(next.dspId && current.session && (!sameWorkspace || opening)));
        setAddress(next);
      }
      fail('');
    };
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, [fail, startTransition]);
  useEffect(() => {
    const verify = () => setReauthenticate(true);
    const required = () => void load();
    window.addEventListener('dispatch-reauthenticate', verify);
    window.addEventListener('dispatch-mfa-required', required);
    window.addEventListener('dispatch-security-changed', required);
    return () => {
      window.removeEventListener('dispatch-reauthenticate', verify);
      window.removeEventListener('dispatch-mfa-required', required);
      window.removeEventListener('dispatch-security-changed', required);
    };
  }, [load]);
  const { route, dspId, page } = address;
  useEffect(() => {
    if (session && !showAuth && !securityRequired)
      void prepareRoute(
        dspId ? 'dsp' : 'platform',
        page,
        {
          session,
          view: dspId ? view : undefined,
        },
        true,
      ).catch(() => undefined);
  }, [session, showAuth, securityRequired, dspId, page, view]);
  useEffect(() => {
    if (!view || !dspId) return;
    const access = { session: session!, view };
    const route = findRoute('dsp', page);
    // A page of a feature the DSP lacks is gone, whether a member was on it when it
    // was switched off or followed a link to it; they go home.
    if (route?.feature && !hasFeature(view, route.feature)) {
      forgetDestination(dspId, page);
      navigate(dspHash(dspId, landing(access)));
      return;
    }
    // Without a landing page, an address naming no page opens the first one they may, in
    // its place, so going back never returns to it. With one, only a malformed address
    // names no page, and it says the page isn't available.
    if (!page && !landingPage()) {
      const first = landing(access);
      if (first) location.replace(dspHash(dspId, first));
      return;
    }
    if (navigation('dsp', access).some((route) => route.id === page))
      rememberDestination(dspId, page as DspRouteId);
  }, [view, dspId, page, session]);
  useBrowserUpdate(Boolean(session) && (!dspId || Boolean(view)) && !switching);
  // A platform owner looking into a DSP is never shown to its team.
  usePresence(session?.user.platformOwner ? undefined : view?.token);
  const reopen = useCallback(async () => {
    if (
      !session ||
      !dspId ||
      showAuth ||
      securityRequired ||
      navigationState.current.session !== session ||
      parseHash(window.location.hash).dspId !== dspId
    )
      return;
    const request = ++admission.current;
    pendingAdmission.current = request;
    const current = () =>
      request === admission.current &&
      navigationState.current.session === session &&
      !navigationState.current.showAuth &&
      !navigationState.current.securityRequired &&
      parseHash(window.location.hash).dspId === dspId;
    // Role changes and expired views immediately stop using the old admission.
    cancelPrefetches();
    credentials(session.csrf);
    setView(undefined);
    setSwitching(true);
    try {
      const next = await openView(session, dspId, current);
      if (!current()) return;
      credentials(session.csrf, next.token);
      // The view is admitted from here, before React commits it. An address change in
      // between, a link followed as the DSP opens, must find it, rather than take the
      // moment for a workspace boundary and drop the view with nothing to reopen it.
      navigationState.current = { ...navigationState.current, view: next };
      void prepareRoute(
        'dsp',
        parseHash(window.location.hash).page,
        { session, view: next },
        true,
      ).catch(() => undefined);
      setView(next);
    } catch (error) {
      if (current()) throw error;
    } finally {
      if (pendingAdmission.current === request) pendingAdmission.current = undefined;
      if (current()) setSwitching(false);
    }
  }, [session, dspId, showAuth, securityRequired]);
  useEffect(() => {
    // Role and membership edits expire every open view of the DSP; reopening
    // picks up the member's new permissions without a manual reload.
    const expired = () => void reopen().catch(() => undefined);
    window.addEventListener('dispatch-view-expired', expired);
    return () => window.removeEventListener('dispatch-view-expired', expired);
  }, [reopen]);
  useEffect(() => {
    const signedOut = () => {
      admission.current++;
      sessionRequest.current++;
      cancelPrefetches();
      credentials('');
      setSession(null);
      setView(undefined);
      setSwitching(false);
    };
    window.addEventListener('dispatch-signed-out', signedOut);
    return () => window.removeEventListener('dispatch-signed-out', signedOut);
  }, []);
  useEffect(() => {
    admission.current++;
    setView(undefined);
    if (!dspId) saveRole();
    if (!session || securityRequired || showAuth) {
      setSwitching(false);
      return;
    }
    credentials(session.csrf);
    if (!dspId) {
      setSwitching(false);
      return;
    }
    void reopen().catch((error) => fail(messageOf(error)));
    return () => {
      admission.current++;
    };
  }, [session, dspId, fail, securityRequired, showAuth, reopen]);
  if (session === undefined)
    return (
      <>
        {sessionError ? <button onClick={() => void load()}>Retry connection</button> : <Loading />}
        <FeedbackMessages />
      </>
    );
  if (showAuth) return <SignedOutScreen key={route} onLogin={() => load(true)} />;
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
  async function logout() {
    await leavePresence();
    await api('/api/auth/logout', {});
    admission.current++;
    sessionRequest.current++;
    cancelPrefetches();
    credentials('');
    setSession(null);
    setView(undefined);
    navigate('');
  }
  return (
    <Shell
      session={session}
      view={view}
      dspId={dspId}
      page={page}
      current={findRoute(scope, page)?.parent ?? page}
      label={routeLabel(scope, page)}
      pending={pending}
      navigation={navigation(scope, { session, view })}
      logout={() => void perform(logout)}
      exitView={() => navigate(platformHash())}
      viewAs={(roleId) => {
        clearNavigationState();
        saveRole(dspId, roleId);
        void perform(reopen);
      }}
    >
      <FeedbackMessages />
      {!online && (
        <p role="status">
          You’re offline. Showing the last loaded data; updates resume when you reconnect.
        </p>
      )}
      {reauthenticate && session && (
        <Modal title="Confirm it’s you" onClose={() => setReauthenticate(false)}>
          <Suspense fallback={<Loading />}>
            <SecurityPrompt
              security={session.security}
              complete={async () => {
                setReauthenticate(false);
                await load(true);
                fail('Verification complete. Retry your action.');
              }}
              signOut={logout}
            />
          </Suspense>
        </Modal>
      )}
      {dspId ? (
        switching ? (
          <Loading />
        ) : view ? (
          <div key={view.token}>
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
const root = createRoot(document.getElementById('root')!);
// Which address this is decides what every link means here, so it is read first.
function start() {
  root.render(<Loading />);
  loadSite().then(
    () =>
      root.render(
        <FeedbackProvider>
          <PageBoundary>
            <Suspense fallback={<Loading />}>
              <App />
            </Suspense>
          </PageBoundary>
        </FeedbackProvider>,
      ),
    () => root.render(<button onClick={start}>Retry connection</button>),
  );
}
start();
