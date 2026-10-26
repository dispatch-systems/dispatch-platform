import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { DspView, SessionView } from '../../../accounts/api/index.js';
import { installFeatures } from '../../frontend/runtime/slots.js';
import { Watchers } from '../../frontend/shell/Watchers.js';

// The page watchers.spec.ts draws: stand-in watchers beside the page, as the shell runs them. One
// runs until it's told to fail; one's code fails to load the first time it's asked for, as with
// the network down; one keeps running. Each shows it runs, as no real watcher does.

/** Runs until it's told to fail. */
function Failing() {
  const [failed, setFailed] = useState(false);
  if (failed) throw new Error('Stand-in failure');
  return <button onClick={() => setFailed(true)}>Fail</button>;
}

let asked = 0;
installFeatures([
  { name: 'failing', watchers: [{ id: 'failing.runs', load: async () => Failing }] },
  {
    name: 'late',
    watchers: [
      {
        id: 'late.loads',
        load: async () => {
          asked += 1;
          if (asked === 1) throw new Error('Stand-in network down');
          return () => <p>Late watcher running</p>;
        },
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
