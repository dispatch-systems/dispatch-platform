import { Suspense, createContext, use, useState, type ReactNode } from 'react';
import { hashQuery, replaceHashQuery } from '../runtime/navigation.js';
import { pageTabs, type DspPageContext, type PageTab } from '../runtime/slots.js';
import { Loading } from '../ui/Loading.js';
import { Tabs } from '../ui/Tabs.js';

// The tabs other owners add to a DSP's page (the pageTabs slot), drawn among the page's own: a
// page with tabs of its own draws them with `usePageTabs`, and the app draws them above any
// other page with `PageWithTabs`. The open one is in the address, `?tab=<id>`, as Settings'
// tabs are, so a link to it keeps working.

/** The open DSP page's context, which the app provides for the tabs other owners add to it. */
export const OpenPage = createContext<DspPageContext | undefined>(undefined);

type Item = readonly [string, ReactNode];

/**
 * A page's tabs with those other owners add to it, after its own, and the one of theirs that
 * is open, if one is. `change` opens the tab chosen in the strip: theirs here, the page's own
 * through `own`.
 */
export function usePageTabs(page: string, own: readonly Item[]) {
  const context = use(OpenPage);
  const hosted = context ? pageTabs(page, context.view) : [];
  const [requested, setRequested] = useState(() => hashQuery().get('tab'));
  const open = hosted.find((tab) => tab.id === requested);
  const select = (id: string | null) => {
    const query = Object.fromEntries(hashQuery());
    delete query.tab;
    replaceHashQuery(id ? { ...query, tab: id } : query);
    setRequested(id);
  };
  return {
    items: [...own, ...hosted.map((tab): Item => [tab.id, tab.label])],
    open,
    change: (id: string, own: (id: string) => void) => {
      const theirs = hosted.find((tab) => tab.id === id);
      select(theirs ? id : null);
      if (!theirs) own(id);
    },
    /** Starts loading another owner's tab as it is about to be chosen. */
    warm: (id: string) =>
      void hosted
        .find((tab) => tab.id === id)
        ?.load()
        .catch(() => undefined),
  };
}

/** Another owner's open tab, its code loading below the page's tab strip. */
export function HostedTab({ tab }: { tab: PageTab }) {
  const context = use(OpenPage);
  return context ? <Suspense fallback={<Loading />}>{tab.render(context)}</Suspense> : null;
}

/** A page without tabs of its own, below the tabs other owners add to it: its own first. */
export function PageWithTabs({
  page,
  label,
  children,
}: {
  page: string;
  label: string;
  children: ReactNode;
}) {
  const tabs = usePageTabs(page, [[page, label]]);
  if (tabs.items.length === 1) return children;
  return (
    <>
      <Tabs
        label={label}
        value={tabs.open?.id ?? page}
        items={tabs.items}
        onIntent={tabs.warm}
        onChange={(id) => tabs.change(id, () => undefined)}
      />
      {tabs.open ? <HostedTab tab={tabs.open} /> : children}
    </>
  );
}
