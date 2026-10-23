import { useEffect, useId, useMemo, useState } from 'react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { useSettings } from '../../../../core/settings/api/client.js';
import { dspHash, navigate } from '../../../../core/shell/frontend/runtime/navigation.js';
import { settingsPage, type SidebarProps } from '../../../../core/shell/frontend/runtime/slots.js';
import { SidebarHead } from '../../../../core/shell/frontend/ui/index.js';
import { moveTo, useAddressedPage } from '../address.js';
import { whoseSettings } from '../drafts.js';
import { search, type Found } from '../search/search.js';
import { drawnPages, drawnSections, loadPage, openPage, prefetchPage } from '../pages.js';
import { PageList } from './PageList.js';
import { Results, resultId } from './Results.js';
import { SearchField } from './SearchField.js';
import '../settings.css';

/**
 * Settings' sidebar, in place of the app's: the way back to the app's, a search of the member's
 * pages, and the pages themselves, under their groups, or, while they search, the pages it found.
 */
export function SettingsSidebar({ session, view, page, back, close }: SidebarProps) {
  // The search outlives the view opening again for the same member, DSP and role viewed as, as
  // Settings' edits do, the sidebar being drawn throughout; for another, it starts afresh.
  const whose = view && whoseSettings(session, view);
  const [search, setSearch] = useState({ whose, query: '' });
  return (
    <div className="settings-sidebar">
      <SidebarHead title="Settings" back={back} />
      {view && whose && (
        <Pages
          key={whose}
          view={view}
          page={page}
          close={close}
          query={search.whose === whose ? search.query : ''}
          setQuery={(query) => setSearch({ whose, query })}
        />
      )}
    </div>
  );
}

function Pages({
  view,
  page,
  close,
  query,
  setQuery,
}: {
  view: DspView;
  page: string;
  close: () => void;
  /** What the member searches for, and what changes it. */
  query: string;
  setQuery: (query: string) => void;
}) {
  const { data: settings } = useSettings();
  const [active, setActive] = useState(0);
  const addressed = useAddressedPage();
  const list = useId();
  const host = settingsPage();
  // A page under Settings, as one it links to, shows its sidebar with none of its pages open.
  const current = settings && page === host ? openPage(settings, view, addressed) : undefined;
  const pages = useMemo(() => (settings ? drawnPages(settings, view) : []), [settings, view]);
  const found = useMemo(
    () => (settings ? search(pages, drawnSections(settings, view), query) : []),
    [settings, view, pages, query],
  );
  const at = Math.min(active, found.length - 1);
  const chosen = found[at] && resultId(list, found[at].page.id);
  useEffect(() => {
    if (chosen) document.getElementById(chosen)?.scrollIntoView({ block: 'nearest' });
  }, [chosen]);
  if (!settings || !host) return null;
  const href = (to: string, section?: string) =>
    dspHash(view.dsp.id, host, section ? { tab: to, find: section } : { tab: to });
  const open = (to: string, section?: string) => {
    close();
    if (page === host) moveTo(to, current, section);
    else navigate(href(to, section));
  };
  const warm = (to: string) => {
    void loadPage(settings, view, to).catch(() => undefined);
    prefetchPage(to, view);
  };
  const choose = (result: Found) => {
    setActive(found.indexOf(result));
    open(result.page.id, result.section);
  };
  return (
    <>
      <SearchField
        query={query}
        change={(next) => {
          setQuery(next);
          setActive(0);
        }}
        list={found.length ? list : undefined}
        active={chosen}
        move={(step) => setActive((at + step + found.length) % found.length)}
        choose={() => found[at] && choose(found[at])}
      />
      <nav className="settings-nav" aria-label="Settings">
        {query.trim() ? (
          found.length > 0 && (
            <Results
              id={list}
              found={found}
              active={at}
              current={current}
              open={choose}
              warm={warm}
            />
          )
        ) : (
          <PageList pages={pages} current={current} href={href} open={open} warm={warm} />
        )}
      </nav>
    </>
  );
}
