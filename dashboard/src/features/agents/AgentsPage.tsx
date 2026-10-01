import { useEffect, useState } from 'react';
import { hashQuery, replaceHashQuery } from '../../app/navigation.js';
import { Header, Tabs } from '../../ui/index.js';
import { ConnectTab } from './ConnectTab.js';
import { KeysTab } from './KeysTab.js';

const tabs = ['keys', 'connect'];
const addressed = () => {
  const tab = hashQuery().get('tab') ?? '';
  return tabs.includes(tab) ? tab : 'keys';
};

/** Platform → Agents: the keys outside agents sign in with, and how to connect one. */
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
          ['keys', 'Keys'],
          ['connect', 'Connect'],
        ]}
      />
      {tab === 'keys' ? <KeysTab /> : <ConnectTab />}
    </>
  );
}
