import { useEffect, useId, useMemo, useState } from 'react';
import { ArrowLeft } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { useSettings } from '../../../../core/settings/api/client.js';
import { dspHash, navigate, wayBack } from '../../../../core/shell/frontend/runtime/navigation.js';
import { settingsPage, type SidebarProps } from '../../../../core/shell/frontend/runtime/slots.js';
import { moveTo, useAddressedPage } from '../address.js';
import { search, type Found } from '../search/search.js';
import { drawnPages, drawnSections, loadPage, openPage, prefetchPage } from '../pages.js';
import { PageList } from './PageList.js';
import { Results, resultId } from './Results.js';
import { SearchField } from './SearchField.js';
import '../settings.css';

/**
 * Settings' sidebar, in place of the app's while Settings is open: the way back to where the
 * member opened it from, a search of their pages, and the pages themselves, under their groups,
 * or, while they search, the pages it found.
 */
export function SettingsSidebar({ view, dspId, page, close }: SidebarProps) {
  return (
    <div className="settings-sidebar">
      <div className="settings-sidebar-head">
        {dspId && (
          <a className="icon-button" href={wayBack(dspId)} aria-label="Back">
            <ArrowLeft aria-hidden="true" />
          </a>
        )}
        <h2>Settings</h2>
      </div>
      {/* Another DSP, or another role viewed as, starts afresh: no search carries over. */}
      {view && <Pages key={view.token} view={view} page={page} close={close} />}
    </div>
  );
}

function Pages({ view, page, close }: { view: DspView; page: string; close: () => void }) {
  const { data: settings } = useSettings();
  const [query, setQuery] = useState('');
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
