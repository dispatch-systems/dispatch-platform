import type { DspSummary } from '../../../../shared/contracts/index.js';
import { setDspFeature } from '../../api/client.js';
import {
  capabilityLabel,
  featureCatalog,
  type FeatureEntry,
} from '../../../shell/frontend/runtime/features.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { Modal } from '../../../shell/frontend/ui/index.js';
import { previewSwitch, sideEffects, switchLabel } from './switches.js';

// Asked only when a switch takes other features with it: names them, one line each with
// why, then acts. A switch that changes nothing else never comes here.
export function FeatureSwitchDialog({
  dsp,
  feature,
  on,
  enabled,
  onClose,
  onDone,
}: {
  dsp: DspSummary;
  feature: FeatureEntry;
  on: boolean;
  enabled: readonly string[];
  onClose: () => void;
  onDone: () => void;
}) {
  const preview = previewSwitch(enabled, feature.id, on);
  const entry = (id: string) => featureCatalog.find((f) => f.id === id)!;
  // The capabilities the page lacks a provider of, when one must be chosen first.
  const unmet = preview
    ? []
    : feature.requires.filter(
        (c) => !featureCatalog.some((f) => f.provides?.includes(c) && enabled.includes(f.id)),
      );
  const others = sideEffects(feature, preview ?? []);
  // The capability two features have in common, or the first one either supplies or needs.
  const shared = (
    a: { provides?: readonly string[]; requires: readonly string[] },
    b: { provides?: readonly string[]; requires: readonly string[] },
  ) =>
    [...(a.provides ?? []), ...a.requires].find(
      (c) => (b.provides ?? []).includes(c) || b.requires.includes(c),
    ) ??
    (a.provides ?? a.requires)[0] ??
    '';
  const why = (change: { feature: string; enabled: boolean }) => {
    // A tab takes only its page along, when it was the page's last one on.
    if (feature.kind === 'tab') return 'has no other tab on';
    const other = entry(change.feature);
    if (on && change.enabled)
      return `${feature.label} needs ${capabilityLabel(shared(feature, other))}`;
    if (on) return `one ${capabilityLabel(shared(feature, other)).replace(/^an? /, '')} at a time`;
    return `needs ${capabilityLabel(shared(other, feature))}`;
  };
  const save = useAction(
    async () => {
      await setDspFeature(dsp.id, feature.id, on);
      onDone();
    },
    { success: `${switchLabel(feature)} switched ${on ? 'on' : 'off'}` },
  );
  return (
    <Modal title={`Switch ${on ? 'on' : 'off'} ${switchLabel(feature)}?`} onClose={onClose}>
      {preview ? (
        <>
          <p>Also switches {others.every((c) => c.enabled) ? 'on' : 'off'}:</p>
          <ul className="dsp-switch-effects">
            {others.map((change) => (
              <li key={change.feature}>
                <strong>{entry(change.feature).label}</strong>
                <span>{why(change)}</span>
              </li>
            ))}
          </ul>
          <div className="form-actions">
            <button type="button" onClick={onClose}>
              Cancel
            </button>
            <button
              className={on ? 'primary' : 'danger'}
              disabled={save.busy}
              onClick={() => void save.run()}
            >
              Switch {on ? 'on' : 'off'}
            </button>
          </div>
        </>
      ) : (
        <>
          <p>
            {feature.label} needs {unmet.map(capabilityLabel).join(' and ')}. Switch on one of its
            connections first.
          </p>
          <div className="form-actions">
            <button type="button" onClick={onClose}>
              Close
            </button>
          </div>
        </>
      )}
    </Modal>
  );
}
