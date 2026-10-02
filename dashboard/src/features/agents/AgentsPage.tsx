import { useEffect, useState } from 'react';
import { hashQuery, replaceHashQuery } from '../../app/navigation.js';
import { Header, Tabs } from '../../ui/index.js';
import { ActivityTab } from './ActivityTab.js';
import { AppsTab } from './AppsTab.js';
import { KeysTab } from './KeysTab.js';

const tabs = ['apps', 'activity', 'keys'];
/** The tab the address names. Connecting an app was a tab of its own, and now starts from
 * Apps. */
const addressed = () => {
  const tab = hashQuery().get('tab') ?? '';
  return tabs.includes(tab) ? tab : 'apps';
};

/** Platform → Agents: the apps signed in with Dispatch and how to connect one, the calls
 * apps and keys make, and the keys outside agents sign in with. */
export function AgentsPage() {
  const [tab, setTab] = useState(addressed);
  // A link to another tab changes only the address's query, which does not remount the page.
  useEffect(() => {
    const changed = () => setTab(addressed());
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, []);
  return (
    <>
      <Header title="Agents" />
      <Tabs
        value={tab}
        label="Agents"
        onChange={(next) => {
          setTab(next);
          replaceHashQuery({ tab: next });
        }}
        items={[
          ['apps', 'Apps'],
          ['activity', 'Activity'],
          ['keys', 'Keys'],
        ]}
      />
      {tab === 'apps' ? <AppsTab /> : tab === 'activity' ? <ActivityTab /> : <KeysTab />}
    </>
  );
}
