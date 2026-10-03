import type { DspView, SessionView } from '../../accounts/api/index.js';
import { connectionFeatures } from '../../shell/frontend/runtime/features.js';
import { ConnectionsPage } from './ConnectionsPage.js';

/** The Connections page as a tab of a DSP's Settings, under the Settings page's heading. */
export function ConnectionsTab({ session, view }: { session: SessionView; view: DspView }) {
  return (
    <div className="settings-connections">
      <ConnectionsPage
        development={session.providerMode === 'fixture'}
        timezone={view.dsp.timezone}
        providers={connectionFeatures(view.features).map((f) => f.id)}
      />
    </div>
  );
}
