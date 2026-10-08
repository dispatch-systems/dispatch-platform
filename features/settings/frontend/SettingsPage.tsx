import { Fragment, Suspense, useEffect, useState, useTransition } from 'react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import { Header, Loading, Tabs } from '../../../core/shell/frontend/ui/index.js';
import { hashQuery, replaceHashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { settingsPieces, type SettingsTab } from '../../../core/shell/frontend/runtime/slots.js';
import { loadBadge, loadTab, loadedBadge, prefetchSettingsTab, visibleTabs } from './tabs.js';

export function SettingsPage({ session, view }: { session: SessionView; view?: DspView }) {
  const tabs = visibleTabs(view);
  const load = (tab?: SettingsTab) => void (tab && loadTab(tab, view).catch(() => undefined));
  const [requestedTab, setTab] = useState(hashQuery().get('tab') || tabs[0]!.id);
  const [pending, startTransition] = useTransition();
  const tabOf = (id: string) => tabs.find((tab) => tab.id === id);
  const tab = tabOf(requestedTab) ?? tabs[0]!;
  // The page's preload brings the badges with it; one the page opened before, or that a
  // newly granted permission shows, is drawn as soon as it arrives.
  const [, badgeLoaded] = useState(0);
  const missing = tabs.filter((each) => each.badge && !loadedBadge(each.id));
  const waiting = missing.map((each) => each.id).join();
  useEffect(() => {
    if (!waiting) return;
    let live = true;
    void Promise.all(missing.map(loadBadge)).then(() => live && badgeLoaded((n) => n + 1));
    return () => {
      live = false;
    };
    // `waiting` names exactly the tabs `missing` holds.
  }, [waiting]);
  return (
    <>
      <Header title="Settings" />
      <Tabs
        value={tab.id}
        onIntent={(value) => {
          load(tabOf(value));
          prefetchSettingsTab(value, view);
        }}
        onChange={(value) => {
          load(tabOf(value));
          prefetchSettingsTab(value, view, true);
          replaceHashQuery({ tab: value });
          startTransition(() => setTab(value));
        }}
        items={tabs.map(({ id, label }) => {
          const Badge = loadedBadge(id);
          return [id, Badge ? <Badge key="label" active={requestedTab === id} /> : label];
        })}
        label="Settings"
      />
      <div aria-busy={pending || undefined} inert={pending}>
        <Suspense fallback={<Loading />}>
          {tab.render({ session, view })}
          {settingsPieces(tab.id, view).map((piece) => (
            <Fragment key={piece.id}>{piece.render({ session, view })}</Fragment>
          ))}
        </Suspense>
      </div>
    </>
  );
}
