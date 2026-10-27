import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { DspView, SessionView } from '../../../accounts/api/index.js';
import { installFeatures } from '../../frontend/runtime/slots.js';
import { Watchers } from '../../frontend/shell/Watchers.js';

// The page watchers.spec.ts draws: stand-in watchers beside the page, as the shell runs them. One
// runs until it's told to fail; one's code never loads; one's code is a chunk of its own, which
// watchers.spec.ts fails to download while the browser is offline; one keeps running. Each shows
// it runs, as no real watcher does.

/** Runs until it's told to fail. */
function Failing() {
  const [failed, setFailed] = useState(false);
  if (failed) throw new Error('Stand-in failure');
  return <button onClick={() => setFailed(true)}>Fail</button>;
}

installFeatures([
  { name: 'failing', watchers: [{ id: 'failing.runs', load: async () => Failing }] },
  {
    name: 'missing',
    watchers: [
      {
        id: 'missing.loads',
        load: () => Promise.reject(new Error('Stand-in download failure')),
      },
    ],
  },
  {
    name: 'late',
    watchers: [
      {
        id: 'late.loads',
        load: () => import('./late-watcher.js').then((module) => module.LateWatcher),
      },
    ],
  },
  {
    name: 'steady',
    watchers: [{ id: 'steady.runs', load: async () => () => <p>Steady watcher running</p> }],
  },
]);

const view = { dsp: { id: 'dsp_stand_in' }, features: [], permissions: [] } as unknown as DspView;
createRoot(document.getElementById('root')!).render(
  <>
    <main>Page</main>
    <Watchers session={{} as SessionView} view={view} />
  </>,
);
