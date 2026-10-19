import {
  Fragment,
  lazy,
  Suspense,
  useEffect,
  useRef,
  useState,
  type ComponentType,
  type ReactNode,
} from 'react';
import { Menu, X, ChevronDown, LogOut, ArrowUpRight, type LucideIcon } from 'lucide-react';
import type { DspView, SessionView } from '../../../accounts/api/index.js';
import { Brand } from '../runtime/Brand.js';
import { Popover } from '../ui/Popover.js';
import { useFocusTrap } from '../ui/useFocusTrap.js';
import { dspHash, platformHash } from '../runtime/navigation.js';
import { sourceLink } from '../lib/source.js';
import {
  accountActions,
  loadedSidebar,
  loadSidebar,
  type DspRouteId,
  type PlatformRouteId,
  type Route,
  type SidebarProps,
} from '../runtime/slots.js';
// Only a platform owner viewing a DSP sees it, so it loads then.
const DspViewBanner = lazy(() =>
  import('./DspViewBanner.js').then((module) => ({ default: module.DspViewBanner })),
);

type Badge = NonNullable<Route['badge']>;
const badges = new WeakMap<Badge, ComponentType<{ active: boolean }>>();
/** A page's badge beside its name, once its code has loaded; one that fails to load is left out. */
function NavBadge({ badge, active }: { badge: Badge; active: boolean }) {
  const [Drawn, setDrawn] = useState(() => badges.get(badge));
  useEffect(() => {
    if (Drawn) return;
    let live = true;
    badge.load().then(
      (loaded) => {
        badges.set(badge, loaded);
        if (live) setDrawn(() => loaded);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [badge, Drawn]);
  return Drawn ? (
    <span className="nav-badge">
      <Drawn active={active} />
    </span>
  ) : null;
}

type Sidebar = NonNullable<Route['sidebar']>;
/**
 * A page's own sidebar, once its code is in, as it is when the page opens; nothing while it
 * loads, and the app's own if it fails to.
 */
function PageSidebar({
  sidebar,
  fallback,
  ...props
}: SidebarProps & { sidebar: Sidebar; fallback: ReactNode }) {
  const [loaded, setLoaded] = useState(() => ({
    of: sidebar,
    Drawn: loadedSidebar(sidebar),
    failed: false,
  }));
  const ours = loaded.of === sidebar;
  useEffect(() => {
    if (ours && loaded.Drawn) return;
    let live = true;
    loadSidebar(sidebar).then(
      (Drawn) => live && setLoaded({ of: sidebar, Drawn, failed: false }),
      () => live && setLoaded({ of: sidebar, Drawn: undefined, failed: true }),
    );
    return () => {
      live = false;
    };
  }, [sidebar, ours, loaded.Drawn]);
  const { Drawn } = loaded;
  if (ours && Drawn) return <Drawn {...props} />;
  return ours && loaded.failed ? fallback : null;
}

export function Shell({
  session,
  view,
  dspId,
  page,
  current,
  label,
  pending = false,
  navigation,
  logout,
  exitView,
  viewAs,
  sidebar,
  children,
}: {
  session: SessionView;
  view?: DspView;
  dspId?: string;
  page: string;
  /** The navigation item the open page belongs to. */
  current: string;
  label: string;
  pending?: boolean;
  navigation: readonly {
    id: string;
    label: string;
    icon?: LucideIcon;
    badge?: Badge;
    preload: () => Promise<unknown>;
  }[];
  logout: () => void;
  exitView: () => void;
  viewAs: (roleId?: string) => void;
  /** The open page's own sidebar, drawn in place of the app's name and pages. */
  sidebar?: Sidebar;
  children: ReactNode;
}) {
  const [mobile, setMobile] = useState(false);
  const [accountAction, setAccountAction] = useState<string>();
  const actions = session.readOnly ? [] : accountActions({ session, view });
  const openedAction = actions.find((action) => action.id === accountAction);
  const drawer = useRef<HTMLElement>(null);
  const name = `${session.user.firstName} ${session.user.lastName}`;
  const workspace = view?.dsp.name ?? (session.user.platformOwner ? 'Platform' : 'Workspace');
  const source = sourceLink(session.source);
  useEffect(() => {
    document.title = `${label} · Dispatch`;
  }, [label]);
  // Navigation closes the drawer; a DSP view that finishes loading behind it does not.
  useEffect(() => setMobile(false), [page, dspId]);
  useFocusTrap(drawer, { active: mobile, onEscape: () => setMobile(false) });
  // The app's name and pages, unless the page open draws a sidebar of its own.
  const pages = (
    <>
      <div className="sidebar-brand">
        <Brand />
        <p>{workspace}</p>
      </div>
      <nav className="nav-list" aria-label="Primary navigation">
        {navigation.map(({ id, label: itemLabel, icon: Icon, badge, preload }) => (
          <a
            key={id}
            href={dspId ? dspHash(dspId, id as DspRouteId) : platformHash(id as PlatformRouteId)}
            className="nav-item"
            aria-current={current === id ? 'page' : undefined}
            onPointerEnter={() => void preload().catch(() => undefined)}
            onPointerDown={() => void preload().catch(() => undefined)}
            onFocus={() => void preload().catch(() => undefined)}
            onClick={() => setMobile(false)}
          >
            {Icon && <Icon aria-hidden="true" />}
            <span>{itemLabel}</span>
            {badge && <NavBadge badge={badge} active={current === id} />}
          </a>
        ))}
      </nav>
    </>
  );
  return (
    <div className="application">
      <a
        href="#main-content"
        className="skip-link"
        onClick={(event) => {
          event.preventDefault();
          document.getElementById('main-content')?.focus();
        }}
      >
        Skip to content
      </a>
      {mobile && (
        <button
          className="navigation-backdrop"
          aria-label="Close navigation"
          onClick={() => setMobile(false)}
        />
      )}
      <aside
        ref={drawer}
        className={`desktop-sidebar ${mobile ? 'navigation-open' : ''}`}
        role={mobile ? 'dialog' : undefined}
        aria-modal={mobile || undefined}
        aria-label={mobile ? 'Navigation' : undefined}
      >
        <button
          className="mobile-navigation-close icon-button"
          aria-label="Close navigation"
          onClick={() => setMobile(false)}
        >
          <X size={18} />
        </button>
        {sidebar ? (
          <PageSidebar
            sidebar={sidebar}
            fallback={pages}
            session={session}
            view={view}
            dspId={dspId}
            page={page}
            close={() => setMobile(false)}
          />
        ) : (
          pages
        )}
        <div className="sidebar-account">
          <Popover
            className="account-menu"
            triggerClassName="account-button"
            label="Account menu"
            trigger={
              <>
                <span className="avatar">
                  {session.user.firstName[0]}
                  {session.user.lastName[0]}
                </span>
                <span className="account-copy">
                  <strong>{name}</strong>
                  <span>
                    {session.user.platformOwner
                      ? `Platform owner${view ? ' · Viewing DSP' : ''}`
                      : view
                        ? view.role.name
                        : 'Team member'}
                  </span>
                </span>
                <Suspense fallback={null}>
                  {actions.map((action) =>
                    action.indicator ? (
                      <Fragment key={action.id}>{action.indicator({ session, view })}</Fragment>
                    ) : null,
                  )}
                </Suspense>
                <ChevronDown aria-hidden="true" />
              </>
            }
          >
            <a href={source.href} target="_blank" rel="noreferrer">
              Source code
              <span className="source-version">
                {source.label}
                <ArrowUpRight aria-hidden="true" />
              </span>
            </a>
            {actions.map(({ id, label: actionLabel, icon: Icon, preload }) => (
              <button
                key={id}
                onPointerEnter={() => void preload().catch(() => undefined)}
                onFocus={() => void preload().catch(() => undefined)}
                onClick={() => {
                  setMobile(false);
                  setAccountAction(id);
                }}
              >
                {Icon && <Icon size={16} />}
                {actionLabel}
              </button>
            ))}
            {!session.readOnly && (
              <button className="danger" onClick={logout}>
                <LogOut size={16} />
                Sign out
              </button>
            )}
          </Popover>
        </div>
      </aside>
      <div className="main-area" inert={mobile}>
        {view && session.user.platformOwner && (
          <Suspense fallback={null}>
            <DspViewBanner
              view={view}
              viewAs={viewAs}
              exitView={exitView}
              readOnly={session.readOnly}
            />
          </Suspense>
        )}
        <header className="topbar">
          <button
            className="mobile-menu icon-button"
            aria-label="Open navigation"
            onClick={() => setMobile(true)}
          >
            <Menu size={20} />
          </button>
          {pending && (
            <span className="navigation-progress" role="status">
              Opening page…
            </span>
          )}
        </header>
        <main
          id="main-content"
          className="page-container"
          tabIndex={-1}
          aria-busy={pending || undefined}
          inert={pending}
        >
          {session.readOnly && (
            <p className="observation-notice" role="status">
              Read-only access · Changes are disabled
            </p>
          )}
          {children}
        </main>
      </div>
      {openedAction && (
        <Suspense fallback={null}>
          {openedAction.render({ session, view, onClose: () => setAccountAction(undefined) })}
        </Suspense>
      )}
    </div>
  );
}
