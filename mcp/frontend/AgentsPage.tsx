import { replaceHashQuery, useHashQuery } from '../../core/shell/frontend/runtime/navigation.js';
import { Header, Tabs } from '../../core/shell/frontend/ui/index.js';
import { ActivityTab } from './ActivityTab.js';
import { AppsTab } from './AppsTab.js';
import { KeysTab } from './KeysTab.js';

const tabs = ['apps', 'activity', 'keys'];
/** The tab the address names. Connecting an app was a tab of its own, and now starts from
 * Apps. */
const addressed = (query: URLSearchParams) => {
  const tab = query.get('tab') ?? '';
  return tabs.includes(tab) ? tab : 'apps';
};

/** Platform → Agents: the apps signed in with Dispatch and how to connect one, the calls
 * apps and keys make, and the keys outside agents sign in with. */
export function AgentsPage() {
  // A link to another tab changes only the address's query, which does not remount the page.
  const [tab, setTab] = useHashQuery(addressed);
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
