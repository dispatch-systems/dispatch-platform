import { useState, useEffect, useLayoutEffect, useCallback, useRef, useTransition } from 'react';
import { flushSync } from 'react-dom';
import type { DspView, SessionView } from '../../core/accounts/api/index.js';
import { useBrowserUpdate } from '../../core/shell/frontend/runtime/browser-update.js';
import { clearNavigationState } from '../../core/shell/frontend/runtime/navigation-state.js';
import {
  api,
  credentials,
  ApiError,
  csrf as activeCsrf,
  view as admittedToken,
} from '../../core/shell/frontend/runtime/api.js';
import { useFeedback } from '../../core/shell/frontend/runtime/feedback.js';
import { askToLeave } from '../../core/shell/frontend/runtime/holds.js';
import {
  dspHash,
  followMoved,
  forgetDestination,
  navigate,
  parseHash,
  rememberDestination,
} from '../../core/shell/frontend/runtime/navigation.js';
import { canNavigateImmediately, findRoute, landing, prepareRoute } from './routes.js';
import { landingPage, listed, type DspRouteId } from '../../core/shell/frontend/runtime/slots.js';
import { messageOf } from '../../core/shell/frontend/lib/errors.js';
import { hasFeature } from '../../core/shell/frontend/runtime/features.js';
import { leavePresence, usePresence } from '../../core/shell/frontend/runtime/presence.js';
import { openView, saveRole } from '../../core/shell/frontend/runtime/session.js';
import { getSession } from '../../core/shell/frontend/runtime/endpoints.js';
import { cancelPrefetches } from '../../core/shell/frontend/runtime/prefetch.js';
import { verified } from '../../core/shell/frontend/runtime/access-recovery.js';
import {
  clearCustomFeatures,
  prepareCustomFeatures,
  setPlatformCustomStyles,
  clearPlatformFeatures,
  preparePlatformFeatures,
} from './private-features.js';

type Session = SessionView;

export function useAppAdmission(
  initialSession: Promise<Session>,
  warmAuth: () => Promise<unknown>,
) {
  const firstSession = useRef<Promise<Session> | undefined>(initialSession);
  const [pending, startTransition] = useTransition();
  const admission = useRef(0);
  const pendingAdmission = useRef<number>(undefined);
  const sessionRequest = useRef(0);
  const [reauthenticate, setReauthenticate] = useState(false);
  const [session, setSession] = useState<Session | null>(),
    [view, setView] = useState<DspView>(),
    [address, setAddress] = useState(() => parseHash(window.location.hash)),
    [switching, setSwitching] = useState(false),
    [sessionError, setSessionError] = useState('');
  const { perform, fail } = useFeedback();
  const securityRequired = Boolean(session?.security.required && !session.security.verified);
  const showAuth =
    session === null ||
    address.route === 'signin' ||
    address.route.startsWith('invite?') ||
    address.route.startsWith('reset?');
  const navigationState = useRef({ address, session, view, showAuth, securityRequired });
  useLayoutEffect(() => {
    navigationState.current = { address, session, view, showAuth, securityRequired };
  }, [address, session, view, showAuth, securityRequired]);
  const load = useCallback(
    async (afterLogin = false) => {
      const request = ++sessionRequest.current;
      admission.current++;
      cancelPrefetches();
      credentials(activeCsrf);
      clearCustomFeatures();
      clearPlatformFeatures();
      setView(undefined);
      setSwitching(
        Boolean(parseHash(window.location.hash).dspId && navigationState.current.session),
      );
      setSessionError('');
      try {
        const reading = firstSession.current ?? getSession();
        firstSession.current = undefined;
        const next = await reading;
        if (request !== sessionRequest.current) return;
        // A signed-out document cannot include an owner's private page sources in its
        // CSP. Signing in or completing MFA needs a fresh document before admission.
        if (
          afterLogin &&
          (!next.security.required || next.security.verified) &&
          (next.platformFrontend || next.dsps.some((dsp) => dsp.customFeatures?.length))
        ) {
          location.reload();
          return;
        }
        const installPlatform = await preparePlatformFeatures(
          next,
          () => request === sessionRequest.current,
        );
        if (request !== sessionRequest.current) return;
        installPlatform?.();
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
    if (!initial.dspId) void warmAuth().catch(() => undefined);
    else void prepareRoute('dsp', initial.page).catch(() => undefined);
    return () => {
      // eslint-disable-next-line react-hooks/exhaustive-deps -- the count ends whatever request is in flight by then
      sessionRequest.current++;
      clearPlatformFeatures();
    };
  }, [load, warmAuth]);
  useEffect(() => {
    const changed = () => {
      followMoved();
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
        if (!opening) {
          setPlatformCustomStyles(false);
          clearCustomFeatures();
        }
        setView(undefined);
        setSwitching(Boolean(next.dspId && current.session && (!sameWorkspace || opening)));
        setAddress(next);
      }
      fail('');
    };
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, [fail, startTransition]);
  // The requests waiting on the prompt are refused when it closes unverified, or with the session.
  const verifying = reauthenticate && Boolean(session);
  useEffect(() => {
    if (!verifying) verified(false);
  }, [verifying]);
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
  const { dspId, page } = address;
  useLayoutEffect(() => {
    setPlatformCustomStyles(
      Boolean(session?.platformFrontend) && !dspId && !showAuth && !securityRequired,
    );
    return () => setPlatformCustomStyles(false);
  }, [session, dspId, showAuth, securityRequired]);
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
    if (route && listed(route, access)) rememberDestination(dspId, page as DspRouteId);
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
    clearCustomFeatures();
    setView(undefined);
    setSwitching(true);
    try {
      const next = await openView(session, dspId, current);
      if (!current()) return;
      const installCustom = await prepareCustomFeatures(next, current);
      if (!current()) return;
      installCustom?.();
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
      clearCustomFeatures();
      clearPlatformFeatures();
      setSession(null);
      setView(undefined);
      setSwitching(false);
    };
    window.addEventListener('dispatch-signed-out', signedOut);
    return () => window.removeEventListener('dispatch-signed-out', signedOut);
  }, []);
  useEffect(() => {
    admission.current++;
    clearCustomFeatures();
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
      // eslint-disable-next-line react-hooks/exhaustive-deps -- the count ends whatever admission is in flight by then
      admission.current++;
      clearCustomFeatures();
    };
  }, [session, dspId, fail, securityRequired, showAuth, reopen]);
  async function logout() {
    await leavePresence();
    await api('/api/auth/logout', {});
    admission.current++;
    sessionRequest.current++;
    cancelPrefetches();
    credentials('');
    clearCustomFeatures();
    clearPlatformFeatures();
    setSession(null);
    setView(undefined);
    navigate('');
  }
  const viewAs = (roleId?: string) =>
    askToLeave(() => {
      clearNavigationState();
      saveRole(dspId, roleId);
      void perform(reopen);
    });
  const closeVerification = () => setReauthenticate(false);
  const completeVerification = () => {
    // Keep the admitted page while its waiting action resumes. It may return one-time
    // recovery codes; account settings refreshes the session once those are saved.
    verified(true);
    setReauthenticate(false);
    return Promise.resolve();
  };
  return {
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
  };
}
