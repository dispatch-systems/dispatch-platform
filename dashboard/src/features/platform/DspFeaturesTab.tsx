import { useState } from 'react';
import { CalendarDays, ClipboardCheck, Plug, Route, Shirt, type LucideIcon } from 'lucide-react';
import type { DspSummary, PageFeature } from '../../../../shared/contracts/index.js';
import { setDspFeature, useDspFeatures } from '../../app/endpoints.js';
import {
  featureCatalog,
  previewSwitch,
  sideEffects,
  switchLabel,
  tabsOf,
  type ConnectionEntry,
  type FeatureEntry,
  type PageEntry,
} from '../../app/features.js';
import { useAction } from '../../app/useAction.js';
import { Badge, ErrorBox } from '../../ui/index.js';
import { FeatureSwitchDialog } from './FeatureSwitchDialog.js';

const icons: Record<PageFeature, LucideIcon> = {
  timecard: CalendarDays,
  uniforms: Shirt,
  routes: Route,
  dvic: ClipboardCheck,
};
const pages = featureCatalog.filter((f): f is PageEntry => f.kind === 'page');
const connections = featureCatalog.filter((f): f is ConnectionEntry => f.kind === 'connection');

// Every page on the left, then the connections; the chosen one's switches on the right.
// A page shows its own switch and its tabs'. A switch acts at once; one that takes other
// features with it asks first.
export function DspFeaturesTab({ dsp, changed }: { dsp: DspSummary; changed: () => void }) {
  const { data, error, refresh } = useDspFeatures(dsp.id);
  const [area, setArea] = useState<PageFeature | 'connections'>(pages[0]!.id);
  const [pending, setPending] = useState<{ feature: FeatureEntry; on: boolean }>();
  // What is switched on, tabs of a page that is off included, so they show as they stay.
  const enabled = data
    ? data.features.filter((state) => state.enabled).map((state) => state.feature)
    : dsp.features;
  const has = (feature: FeatureEntry) => enabled.includes(feature.id);
  const done = () => {
    refresh();
    changed();
  };
  const direct = useAction(
    async (feature: FeatureEntry, on: boolean) => {
      await setDspFeature(dsp.id, feature.id, on);
      done();
    },
    { success: (feature, on) => `${switchLabel(feature)} switched ${on ? 'on' : 'off'}` },
  );
  const toggle = (feature: FeatureEntry, on: boolean) => {
    const preview = previewSwitch(enabled, feature.id, on);
    // Alone, or already as asked: no question to ask.
    if (preview && !sideEffects(feature, preview).length) {
      if (preview.length) void direct.run(feature, on);
      return;
    }
    setPending({ feature, on });
  };
  const row = (feature: FeatureEntry, label = feature.label, locked = false) => (
    <div className={`dsp-feature-row ${has(feature) && !locked ? '' : 'off'}`} key={feature.id}>
      <input
        type="checkbox"
        role="switch"
        aria-label={feature.kind === 'tab' ? switchLabel(feature) : label}
        checked={has(feature)}
        disabled={!data || direct.busy || locked}
        onChange={(event) => toggle(feature, event.target.checked)}
      />
      <strong>{label}</strong>
      {feature.kind === 'connection' && has(feature) && (
        <Badge value={dsp.connections[feature.id] ?? 'not_connected'} />
      )}
    </div>
  );
  const count = (features: FeatureEntry[]) => features.filter(has).length;
  const page = pages.find((candidate) => candidate.id === area);
  const tabs = page ? tabsOf(page.id) : [];
  const title = page?.label ?? 'Connections';
  return (
    <div className="dsp-features">
      <ErrorBox message={error} />
      <nav className="dsp-areas" aria-label="Feature areas">
        <h3 className="dsp-group">Pages</h3>
        {pages.map((candidate) => {
          const Icon = icons[candidate.id];
          const own = tabsOf(candidate.id);
          return (
            <button
              key={candidate.id}
              className={has(candidate) ? undefined : 'off'}
              aria-current={candidate.id === area ? 'true' : undefined}
              onClick={() => setArea(candidate.id)}
            >
              <Icon size={16} aria-hidden="true" />
              <span>{candidate.label}</span>
              <small>
                {!has(candidate) ? 'Off' : own.length ? `${count(own)}/${own.length}` : 'On'}
              </small>
            </button>
          );
        })}
        <h3 className="dsp-group">Connections</h3>
        <button
          aria-current={area === 'connections' ? 'true' : undefined}
          onClick={() => setArea('connections')}
        >
          <Plug size={16} aria-hidden="true" />
          <span>Connections</span>
          <small>
            {count(connections)}/{connections.length}
          </small>
        </button>
      </nav>
      <section className="dsp-area" aria-label={title}>
        <h3>
          {title}
          {page ? (
            tabs.length > 0 && (
              <span>
                {count(tabs)} of {tabs.length} tabs
              </span>
            )
          ) : (
            <span>
              {count(connections)} of {connections.length}
            </span>
          )}
        </h3>
        {page ? (
          <>
            {row(page, `${page.label} page`)}
            {tabs.length > 0 && <h4 className="dsp-tabs-label">Tabs</h4>}
            {tabs.map((tab) => row(tab, tab.label, !has(page)))}
          </>
        ) : (
          connections.map((connection) => row(connection))
        )}
      </section>
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
