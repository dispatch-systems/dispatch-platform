import { useState } from 'react';
import { ChevronRight, Eye, EyeOff, Plug } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import { setDspFeature, showDspFeature, useDspFeatures } from '../../api/client.js';
import {
  featureCatalog,
  subsOf,
  type ConnectionEntry,
  type FeatureEntry,
  type PageEntry,
} from '../../../shell/frontend/runtime/features.js';
import { switchIcon } from '../../../shell/frontend/runtime/slots.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { Badge, ErrorBox } from '../../../shell/frontend/ui/index.js';
import { FeatureSwitchDialog } from './FeatureSwitchDialog.js';
import { previewSwitch, sideEffects, switchLabel } from './switches.js';

const pages = featureCatalog.filter((f): f is PageEntry => f.kind === 'page');
const connections = featureCatalog.filter((f): f is ConnectionEntry => f.kind === 'connection');

// Every feature, a row each with its switch, and the parts of one with any under it when it
// is opened; then the connections. A mandatory feature or part shows its switch on and greyed
// out: every DSP has it. A part can't be switched while its feature is off, and shows off;
// switching the feature back on brings each part back as it was. A switch acts at once; one
// that takes other features with it asks first. The eye beside an optional one hides it from
// the DSP's members while it keeps running, as the Platform Owner view still shows it; a
// hidden feature hides its parts too.
export function DspFeaturesTab({ dsp, changed }: { dsp: DspSummary; changed: () => void }) {
  const { data, error, refresh } = useDspFeatures(dsp.id);
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const [pending, setPending] = useState<{ feature: FeatureEntry; on: boolean }>();
  // What is switched on, parts of a feature that is off included, so they come back as they were.
  const enabled = data
    ? data.features.filter((state) => state.enabled).map((state) => state.feature)
    : dsp.features;
  const switched = (id: string) => enabled.includes(id);
  const hidden = (feature: FeatureEntry) =>
    data?.features.some((state) => state.feature === feature.id && !state.shown) ?? false;
  // What the DSP has: a part only while its feature is on too.
  const has = (feature: FeatureEntry) =>
    switched(feature.id) && (feature.kind !== 'sub' || switched(feature.page));
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
  const seeing = useAction(
    async (feature: FeatureEntry, shown: boolean) => {
      await showDspFeature(dsp.id, feature.id, shown);
      done();
    },
    {
      success: (feature, shown) =>
        `${switchLabel(feature)} ${shown ? 'shown to' : 'hidden from'} ${dsp.name}`,
    },
  );
  // What every DSP has is always shown; an optional one can be hidden.
  const eye = (feature: FeatureEntry) =>
    feature.mandatory ? (
      <span className="dsp-eye" aria-hidden="true" />
    ) : (
      <button
        className="dsp-eye"
        aria-pressed={hidden(feature)}
        aria-label={`${hidden(feature) ? 'Show' : 'Hide'} ${switchLabel(feature)} ${
          hidden(feature) ? 'to' : 'from'
        } the DSP`}
        title={hidden(feature) ? 'Hidden from the DSP: it keeps running' : 'Shown to the DSP'}
        disabled={!data || seeing.busy}
        onClick={() => void seeing.run(feature, hidden(feature))}
      >
        {hidden(feature) ? (
          <EyeOff size={16} aria-hidden="true" />
        ) : (
          <Eye size={16} aria-hidden="true" />
        )}
      </button>
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
  const expand = (page: PageEntry) =>
    setOpen((shown) => {
      const next = new Set(shown);
      if (next.has(page.id)) next.delete(page.id);
      else next.add(page.id);
      return next;
    });
  const control = (feature: FeatureEntry, locked = false) => (
    <input
      type="checkbox"
      role="switch"
      aria-label={switchLabel(feature)}
      title={feature.mandatory ? 'Every DSP has it' : undefined}
      checked={has(feature)}
      disabled={!data || direct.busy || locked || Boolean(feature.mandatory)}
      onChange={(event) => toggle(feature, event.target.checked)}
    />
  );
  return (
    <div className="dsp-features">
      <ErrorBox message={error} />
      <section aria-label="Features">
        <h3 className="dsp-group">Features</h3>
        <ul className="dsp-feature-list">
          {pages.map((page) => {
            const Icon = switchIcon(page.id);
            const parts = subsOf(page.id);
            const shown = open.has(page.id);
            return (
              <li key={page.id}>
                <div className={`dsp-feature-row ${has(page) ? '' : 'off'}`}>
                  {parts.length ? (
                    <button
                      className="dsp-expand"
                      aria-expanded={shown}
                      aria-label={`${page.label}'s parts`}
                      onClick={() => expand(page)}
                    >
                      <ChevronRight size={16} aria-hidden="true" />
                    </button>
                  ) : (
                    <span className="dsp-expand" aria-hidden="true" />
                  )}
                  {control(page)}
                  {Icon && <Icon size={16} aria-hidden="true" />}
                  <strong>{page.label}</strong>
                  <small>
                    {page.mandatory
                      ? 'Every DSP'
                      : hidden(page)
                        ? 'Hidden from DSP'
                        : parts.length && has(page)
                          ? `${parts.filter(has).length} of ${parts.length} parts`
                          : ''}
                  </small>
                  {eye(page)}
                </div>
                {shown && (
                  <ul className="dsp-parts" aria-label={`${page.label}'s parts`}>
                    {parts.map((part) => (
                      <li key={part.id} className={`dsp-feature-row ${has(part) ? '' : 'off'}`}>
                        {control(part, !has(page))}
                        <strong>{part.label}</strong>
                        <small>
                          {part.mandatory
                            ? 'Every DSP'
                            : hidden(part)
                              ? 'Hidden from DSP'
                              : part.tab
                                ? 'Tab'
                                : ''}
                        </small>
                        {eye(part)}
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            );
          })}
        </ul>
      </section>
      <section aria-label="Connections">
        <h3 className="dsp-group">
          <Plug size={14} aria-hidden="true" />
          Connections
        </h3>
        <ul className="dsp-feature-list">
          {connections.map((connection) => (
            <li key={connection.id} className={`dsp-feature-row ${has(connection) ? '' : 'off'}`}>
              <span className="dsp-expand" aria-hidden="true" />
              {control(connection)}
              <strong>{connection.label}</strong>
              {has(connection) && (
                <Badge value={dsp.connections[connection.id] ?? 'not_connected'} />
              )}
            </li>
          ))}
        </ul>
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
