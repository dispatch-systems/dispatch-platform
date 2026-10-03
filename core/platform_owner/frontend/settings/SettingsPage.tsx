import { Suspense, useState, useTransition } from 'react';
import type { SessionView } from '../../../accounts/api/index.js';
import { Header, Loading, Tabs } from '../../../shell/frontend/ui/index.js';
import { hashQuery, replaceHashQuery } from '../../../shell/frontend/runtime/navigation.js';
import { tabOf, tabs } from './tabs.js';

const load = (id: string) =>
  void tabOf(id)
    ?.load()
    .catch(() => undefined);

/** The platform owner's own Settings. */
export function SettingsPage({ session }: { session: SessionView }) {
  const [requestedTab, setTab] = useState(hashQuery().get('tab') || tabs[0].id);
  const [pending, startTransition] = useTransition();
  const tab = tabOf(requestedTab) ?? tabs[0];
  return (
    <>
      <Header title="Settings" />
      <Tabs
        value={tab.id}
        onIntent={load}
        onChange={(value) => {
          load(value);
          replaceHashQuery({ tab: value });
          startTransition(() => setTab(value));
        }}
        items={tabs.map(({ id, label }) => [id, label])}
        label="Settings"
      />
      <div aria-busy={pending || undefined} inert={pending}>
        <Suspense fallback={<Loading />}>{tab.render({ session })}</Suspense>
      </div>
    </>
  );
}
