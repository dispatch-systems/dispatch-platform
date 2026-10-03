import { Suspense, useState, useTransition } from 'react';
import type { DspView, SessionView } from '../../../shared/contracts/index.js';
import { Header, Loading, Tabs } from '../../../core/shell/frontend/ui/index.js';
import { hashQuery, replaceHashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import type { SettingsTab } from '../../../core/shell/frontend/runtime/slots.js';
import { prefetchSettingsTab, visibleTabs } from './tabs.js';

const load = (tab?: SettingsTab) => void tab?.load().catch(() => undefined);

export function SettingsPage({ session, view }: { session: SessionView; view?: DspView }) {
  const tabs = visibleTabs(view);
  const [requestedTab, setTab] = useState(hashQuery().get('tab') || tabs[0]!.id);
  const [pending, startTransition] = useTransition();
  const tabOf = (id: string) => tabs.find((tab) => tab.id === id);
  const tab = tabOf(requestedTab) ?? tabs[0]!;
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
        items={tabs.map(({ id, label, badge }) => [
          id,
          badge ? (
            <Suspense key="label" fallback={label}>
              <badge.Label active={requestedTab === id} />
            </Suspense>
          ) : (
            label
          ),
        ])}
        label="Settings"
      />
      <div aria-busy={pending || undefined} inert={pending}>
        <Suspense fallback={<Loading />}>{tab.render({ session, view })}</Suspense>
      </div>
    </>
  );
}
