import { useState } from 'react';
import { AppWindow, Plug, type LucideIcon } from 'lucide-react';
import type { DspSummary } from '../../../../shared/contracts/index.js';
import { setDspFeature, useDspFeatures } from '../../app/endpoints.js';
import { featureCatalog, previewSwitch, type FeatureEntry } from '../../app/features.js';
import { useAction } from '../../app/useAction.js';
import { Badge, ErrorBox } from '../../ui/index.js';
import { FeatureSwitchDialog } from './FeatureSwitchDialog.js';

type Area = { kind: FeatureEntry['kind']; label: string; icon: LucideIcon };
const areas: Area[] = [
  { kind: 'page', label: 'Pages', icon: AppWindow },
  { kind: 'connection', label: 'Connections', icon: Plug },
];

// The catalog by area: choose an area on the left, switch its features on the right.
// A switch acts at once; one that takes other features with it asks first.
export function DspFeaturesTab({ dsp, changed }: { dsp: DspSummary; changed: () => void }) {
  const { data, error, refresh } = useDspFeatures(dsp.id);
  const [kind, setKind] = useState<Area['kind']>('page');
  const [pending, setPending] = useState<{ feature: FeatureEntry; on: boolean }>();
  const enabled = data
    ? data.features.filter((state) => state.enabled).map((state) => state.feature)
    : dsp.features;
  const of = (area: Area) => featureCatalog.filter((feature) => feature.kind === area.kind);
  const on = (features: FeatureEntry[]) => features.filter((f) => enabled.includes(f.id)).length;
  const area = areas.find((candidate) => candidate.kind === kind)!;
  const items = of(area);
  const done = () => {
    refresh();
    changed();
  };
  const direct = useAction(
    async (feature: FeatureEntry, on: boolean) => {
      await setDspFeature(dsp.id, feature.id, on);
      done();
    },
    { success: (feature, on) => `${feature.label} switched ${on ? 'on' : 'off'}` },
  );
  const toggle = (feature: FeatureEntry, on: boolean) => {
    const preview = previewSwitch(enabled, feature.id, on);
    // Alone, or already as asked: no question to ask.
    if (preview && preview.every((change) => change.feature === feature.id)) {
      if (preview.length) void direct.run(feature, on);
      return;
    }
    setPending({ feature, on });
  };
  return (
    <div className="dsp-features">
      <ErrorBox message={error} />
      <div className="dsp-areas" role="tablist" aria-label="Feature areas">
        {areas.map((candidate) => (
          <button
            key={candidate.kind}
            role="tab"
            aria-selected={candidate.kind === kind}
            onClick={() => setKind(candidate.kind)}
          >
            <candidate.icon size={16} aria-hidden="true" />
            <span>{candidate.label}</span>
            <small>
              {on(of(candidate))}/{of(candidate).length}
            </small>
          </button>
        ))}
      </div>
      <div className="dsp-area" role="tabpanel" aria-label={area.label}>
        <h3>
          {area.label}
          <span>
            {on(items)} of {items.length}
          </span>
        </h3>
        {items.map((feature) => {
          const has = enabled.includes(feature.id);
          return (
            <div className={`dsp-feature-row ${has ? '' : 'off'}`} key={feature.id}>
              <input
                type="checkbox"
                role="switch"
                aria-label={feature.label}
                checked={has}
                disabled={!data || direct.busy}
                onChange={(event) => toggle(feature, event.target.checked)}
              />
              <strong>{feature.label}</strong>
              {feature.kind === 'connection' && has && (
                <Badge value={dsp.connections[feature.id] ?? 'not_connected'} />
              )}
            </div>
          );
        })}
      </div>
      {pending && data && (
        <FeatureSwitchDialog
          dsp={dsp}
          feature={pending.feature}
          on={pending.on}
          enabled={enabled}
          onClose={() => setPending(undefined)}
          onDone={() => {
            setPending(undefined);
            done();
          }}
        />
      )}
    </div>
  );
}
