import {
  Fragment,
  lazy,
  Suspense,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { Menu, X, ChevronDown, LogOut, ArrowUpRight, type LucideIcon } from 'lucide-react';
import type { DspView, SessionView } from '../../../accounts/api/index.js';
import { Brand } from '../runtime/Brand.js';
import { PageBadge } from '../ui/PageBadge.js';
import { Popover } from '../ui/Popover.js';
import { SidebarHead } from '../ui/SidebarHead.js';
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
  type Sidebar,
  type SidebarProps,
} from '../runtime/slots.js';
import { SidebarPanels } from './SidebarPanels.js';
// Only a platform owner viewing a DSP sees it, so it loads then.
const DspViewBanner = lazy(() =>
  import('./DspViewBanner.js').then((module) => ({ default: module.DspViewBanner })),
);

type Badge = NonNullable<Route['badge']>;
/**
 * An item of a sidebar's list: a page, or in the app's, a sidebar of pages, which opens in place
 * of the app's with the pages it lists.
 */
export type NavItem = {
  id: string;
  label: string;
  icon?: LucideIcon;
  badge?: Badge;
  /** Warms what opening it needs, as it is pointed at. */
  preload: () => Promise<unknown>;
  sidebar?: Sidebar;
  pages?: readonly NavItem[];
};

/**
 * A sidebar's list, `current` highlighted: a link to each page, and in the app's (`open`) a
 * button for each sidebar of pages, which opens it. Following a link closes the sidebar where it
 * opens over the page (`close`).
 */
function NavList({
  label,
  items,
  current,
  dspId,
  open,
  close,
}: {
  label: string;
  items: readonly NavItem[];
  current: string;
  dspId?: string;
  open?: (sidebar: Sidebar) => void;
  close: () => void;
}) {
  return (
    <nav className={open ? 'nav-list app-pages' : 'nav-list'} aria-label={label}>
      {items.map(({ id, label: itemLabel, icon: Icon, badge, preload, sidebar }) => {
        const warm = () => void preload().catch(() => undefined);
        const item = {
          className: 'nav-item',
          'aria-current': current === id ? ('page' as const) : undefined,
          onPointerEnter: warm,
          onPointerDown: warm,
          onFocus: warm,
        };
        const named = (
          <>
            {Icon && <Icon aria-hidden="true" />}
            <span>{itemLabel}</span>
            {badge && <PageBadge badge={badge} active={current === id} />}
          </>
        );
        return sidebar && open ? (
          <button key={id} type="button" data-opens={id} onClick={() => open(sidebar)} {...item}>
            {named}
          </button>
        ) : (
          <a
            key={id}
            href={dspId ? dspHash(dspId, id as DspRouteId) : platformHash(id as PlatformRouteId)}
            onClick={close}
            {...item}
          >
            {named}
          </a>
        );
      })}
    </nav>
  );
}

type Drawing = NonNullable<Sidebar['draw']>;
/**
 * What a sidebar of pages draws of its own, once its code is in, as it is when it opens: its
 * head while it loads (`loading`), and the shell's list of its pages if it fails to (`fallback`).
 */
function DrawnSidebar({
  drawing,
  loading,
  fallback,
  ...props
}: SidebarProps & { drawing: Drawing; loading: ReactNode; fallback: ReactNode }) {
  const [loaded, setLoaded] = useState(() => ({
    of: drawing,
    Drawn: loadedSidebar(drawing),
    failed: false,
  }));
  const ours = loaded.of === drawing;
  useEffect(() => {
    if (ours && loaded.Drawn) return;
    let live = true;
    loadSidebar(drawing).then(
      (Drawn) => live && setLoaded({ of: drawing, Drawn, failed: false }),
      () => live && setLoaded({ of: drawing, Drawn: undefined, failed: true }),
    );
    return () => {
      live = false;
    };
  }, [drawing, ours, loaded.Drawn]);
  const { Drawn } = loaded;
  if (ours && Drawn) return <Drawn {...props} />;
  return ours && loaded.failed ? fallback : loading;
}

export function Shell({
  session,
  view,
  dspId,
  page,
  current,
  label,
  pending = false,
  fills = false,
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
  /** The item of the app's sidebar the open page belongs to. */
  current: string;
  label: string;
  pending?: boolean;
  /** Whether the open page fills the window, its parts scrolling within it. */
  fills?: boolean;
  /** The app's sidebar: its pages, and its sidebars of pages with theirs. */
  navigation: readonly NavItem[];
  logout: () => void;
  exitView: () => void;
  viewAs: (roleId?: string) => void;
  /** The sidebar of pages the open page is under, which the sidebar shows as the page opens. */
  sidebar?: Sidebar;
  children: ReactNode;
}) {
  const [mobile, setMobile] = useState(false);
  const [accountAction, setAccountAction] = useState<string>();
  const actions = session.readOnly ? [] : accountActions({ session, view });
  const openedAction = actions.find((action) => action.id === accountAction);
  const drawer = useRef<HTMLElement>(null);
  const name = `${session.user.firstName} ${session.user.lastName}`;
  const source = sourceLink(session.source);
  useEffect(() => {
    document.title = `${label} · Dispatch`;
  }, [label]);
  // Navigation closes the drawer; a DSP view that finishes loading behind it does not.
  useEffect(() => setMobile(false), [page, dspId]);
  // The drawer is there only while the menu button is (shell.css, up to 980px wide): widening
  // the window past it closes the drawer, so the page behind it isn't left inert.
  useEffect(() => {
    if (!mobile) return;
    const narrow = matchMedia('(width <= 980px)');
    const widened = () => narrow.matches || setMobile(false);
    narrow.addEventListener('change', widened);
    return () => narrow.removeEventListener('change', widened);
  }, [mobile]);
  useFocusTrap(drawer, { active: mobile, onEscape: () => setMobile(false) });
  const close = () => setMobile(false);
  // The list the sidebar shows: as a page opens, the sidebar of pages it is under, or else the
  // app's; then whichever the person opens, or goes back to, the page staying open.
  const opened = `${dspId ?? ''}/${page}`;
  const [list, setList] = useState({ opened, under: sidebar, shown: sidebar });
  if (list.opened !== opened || list.under !== sidebar)
    setList({ opened, under: sidebar, shown: sidebar });
  // The latest the person asked to open, which one opened before it may no longer show, nor one
  // asked for on a page another has opened in place of since, nor the one shown already.
  const opening = useRef(0);
  const drawn = useRef({ opened, shown: list.shown });
  useLayoutEffect(() => {
    drawn.current = { opened, shown: list.shown };
  });
  // Where the focus goes as the person opens a list or goes back: into the one shown, or to the
  // item of the app's that opened the one left.
  const focusing = useRef<{ from?: Sidebar }>(undefined);
  const open = (to: Sidebar) => {
    // While a page opens, the sidebar is that page's to show once it is open.
    if (pending) return;
    const asked = ++opening.current;
    const show = () => {
      const now = drawn.current;
      if (asked !== opening.current || now.opened !== opened || now.shown === to) return;
      focusing.current = {};
      setList((then) => ({ ...then, shown: to }));
    };
    // What it draws of its own is in before it slides in, so it slides in whole; failing to
    // load, the shell's list of its pages slides in instead.
    if (to.draw && !loadedSidebar(to.draw)) loadSidebar(to.draw).then(show, show);
    else show();
  };
  const back = () => {
    opening.current++;
    focusing.current = { from: list.shown };
    setList((now) => ({ ...now, shown: undefined }));
  };
  useLayoutEffect(() => {
    const to = focusing.current;
    focusing.current = undefined;
    const shown = drawer.current?.querySelector('.sidebar-panel:not([inert])');
    if (!to || !shown) return;
    // The item that opened it may not be listed, as for a sidebar a link opened: the first then.
    const opener =
      to.from && shown.querySelector<HTMLElement>(`[data-opens="${CSS.escape(to.from.id)}"]`);
    (opener || shown.querySelector<HTMLElement>('a[href], button, input'))?.focus({
      preventScroll: true,
    });
  }, [list.shown]);
  // The app's sidebar.
  const pages = (
    <NavList
      label="Primary navigation"
      items={navigation}
      current={sidebar?.id ?? current}
      dspId={dspId}
      open={open}
      close={close}
    />
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
        <div className="sidebar-brand">
          <Brand />
        </div>
        <SidebarPanels
          shown={list.shown}
          draw={(panel) => {
            if (!panel) return pages;
            const head = <SidebarHead title={panel.label} back={back} />;
            // The shell's list of its pages, under its way back and its name: the page open
            // highlighted, or the one it is under, as a page of detail is.
            const items = navigation.find((item) => item.sidebar === panel)?.pages ?? [];
            const plainList = (
              <>
                {head}
                <NavList
                  label={panel.label}
                  items={items}
                  current={items.some((item) => item.id === page) ? page : current}
                  dspId={dspId}
                  close={close}
                />
              </>
            );
            return panel.draw ? (
              <DrawnSidebar
                drawing={panel.draw}
                loading={head}
                fallback={plainList}
                session={session}
                view={view}
                dspId={dspId}
                page={page}
                back={back}
                close={close}
              />
            ) : (
              plainList
            );
          }}
        />
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
      <div className={fills ? 'main-area fills' : 'main-area'} inert={mobile}>
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
