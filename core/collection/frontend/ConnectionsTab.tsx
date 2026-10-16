import type { DspView, SessionView } from '../../accounts/api/index.js';
import { connectionFeatures } from '../../shell/frontend/runtime/features.js';
import { can } from '../../shell/frontend/runtime/permissions.js';
import { connectionPieces } from '../../shell/frontend/runtime/slots.js';
import { ConnectionsPage } from './ConnectionsPage.js';
import { dspPieces } from './dspPieces.js';

/**
 * The Connections page as a tab of a DSP's Settings, under the Settings page's heading: the
 * DSP's own accounts for those who manage them, a card that says so for others too, and
 * everyone's own.
 */
export function ConnectionsTab({ session, view }: { session: SessionView; view: DspView }) {
  const manages = can(view, 'connections.manage');
  const dsp = dspPieces(view);
  return (
    <div className="settings-connections">
      <ConnectionsPage
        development={session.providerMode === 'fixture'}
        timezone={view.dsp.timezone}
        providers={manages ? connectionFeatures(view.features).map((f) => f.id) : []}
        dsp={dsp}
        adds={manages ? dsp.filter((piece) => piece.adds) : []}
        personal={connectionPieces('personal', view)}
        context={{ session, view }}
      />
    </div>
  );
}
