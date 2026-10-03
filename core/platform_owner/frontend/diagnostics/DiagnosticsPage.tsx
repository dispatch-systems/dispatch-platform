import { useEffect, useMemo, useState } from 'react';
import {
  hashQuery,
  parseHash,
  replaceHashQuery,
} from '../../../shell/frontend/runtime/navigation.js';
import { useData } from '../../../shell/frontend/runtime/api.js';
import { usePlatformJobs, usePlatformHealth } from '../../api/client.js';
import { DataState, ErrorBox, Header, Tabs } from '../../../shell/frontend/ui/index.js';
import { collectionHistory } from './collection-history.js';
import type { Diagnostics } from './types.js';
import { DiagnosticsCollections } from './DiagnosticsCollections.js';
import { DiagnosticsEmail } from './DiagnosticsEmail.js';
import { DiagnosticsOverview } from './DiagnosticsOverview.js';
import { DiagnosticsTestDsps } from './DiagnosticsTestDsps.js';

const tabs = ['overview', 'collections', 'email', 'test-dsps'];
function addressed() {
  const query = hashQuery();
  const tab = query.get('tab') ?? '';
  return {
    tab: tabs.includes(tab) ? tab : 'overview',
    source: query.get('source') ?? '',
    run: query.get('run') ?? '',
  };
}

export function DiagnosticsPage() {
  const [place, setPlace] = useState(addressed);
  const health = usePlatformHealth(['overview', 'email'].includes(place.tab) ? 10000 : 0);
  const diagnostics = useData<Diagnostics>(
    '/api/platform/diagnostics',
    ['overview', 'test-dsps'].includes(place.tab) ? 15000 : 0,
  );
  const jobs = usePlatformJobs(['overview', 'collections'].includes(place.tab) ? 5000 : -1);
  const sources = useMemo(() => collectionHistory(jobs.data ?? []), [jobs.data]);
  // A link to another tab changes only the address's query, which does not remount the page.
  useEffect(() => {
    const changed = () => {
      const target = parseHash(window.location.hash);
      if (!target.dspId && target.page === 'jobs') setPlace(addressed());
    };
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, []);
  const go = (tab: string, source = place.source, run = '') => {
    setPlace({ tab, source, run });
    replaceHashQuery({ tab, ...(source && { source }), ...(run && { run }) });
  };
  const attention = sources.filter((source) => source.warnings.length).length;
  const mailFailed = health.data?.mail.failed ?? 0;
  const overview =
    health.data && diagnostics.data && jobs.data
      ? { health: health.data, diagnostics: diagnostics.data, jobs: jobs.data }
      : undefined;
  const error =
    place.tab === 'overview'
      ? diagnostics.error || health.error || jobs.error
      : place.tab === 'email'
        ? health.error
        : place.tab === 'collections'
          ? jobs.error
          : diagnostics.error;
  const retry = () => {
    health.refresh();
    diagnostics.refresh();
    jobs.refresh();
  };
  const count = (text: string, value: number) => (
    <>
      {text}
      {value > 0 && <span className="tab-count">{value}</span>}
    </>
  );
  return (
    <>
      <Header title="Diagnostics" />
      <Tabs
        value={place.tab}
        onChange={(tab) => go(tab)}
        label="Diagnostics"
        items={[
          ['overview', 'Overview'],
          ['collections', count('Collections', attention)],
          ['email', count('Email', mailFailed)],
          ['test-dsps', 'Test DSPs'],
        ]}
      />
      <ErrorBox message={error} />
      {place.tab === 'overview' && (
        <DataState
          data={overview}
          failed={Boolean(health.error || diagnostics.error || jobs.error)}
          retry={retry}
        >
          {(data) => (
            <DiagnosticsOverview
              health={data.health}
              diagnostics={data.diagnostics}
              jobs={data.jobs}
              sources={sources}
              openSource={(source, run) => go('collections', source, run)}
              openEmail={() => go('email')}
            />
          )}
        </DataState>
      )}
      {place.tab === 'collections' && (
        <DataState data={jobs.data} failed={Boolean(jobs.error)} retry={jobs.refresh}>
          {() => (
            <DiagnosticsCollections
              key={place.run}
              sources={sources}
              selected={place.source}
              run={place.run}
              onSelect={(source) => go('collections', source)}
            />
          )}
        </DataState>
      )}
      {place.tab === 'email' && (
        <DataState data={health.data} failed={Boolean(health.error)} retry={health.refresh}>
          {(data) => <DiagnosticsEmail mail={data.mail} onChanged={health.refresh} />}
        </DataState>
      )}
      {place.tab === 'test-dsps' && (
        <DataState
          data={diagnostics.data}
          failed={Boolean(diagnostics.error)}
          retry={diagnostics.refresh}
        >
          {(data) => <DiagnosticsTestDsps diagnostics={data} refresh={diagnostics.refresh} />}
        </DataState>
      )}
    </>
  );
}
