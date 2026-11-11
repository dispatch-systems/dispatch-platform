import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import type { Visibility } from '../../api/index.js';
import {
  setDspFeature,
  setDspVisibility,
  showDspAiSupport,
  useDspFeatures,
} from '../../api/client.js';
import {
  featureLabel,
  subsOf,
  type ConnectionEntry,
  type FeatureEntry,
  type PageEntry,
} from '../../../shell/frontend/runtime/features.js';
import { featureCatalog as publicCatalog } from '../../../tenancy/api/generated/access-catalog.js';
import { useUpdateState } from '../../../shell/frontend/runtime/navigation-state.js';
import {
  hashQuery,
  parseHash,
  replaceHashQuery,
} from '../../../shell/frontend/runtime/navigation.js';
import { useChanges } from '../../../shell/frontend/runtime/changes.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { Badge, Empty, ErrorBox, SearchInput } from '../../../shell/frontend/ui/index.js';
import { FeatureSwitchDialog } from './FeatureSwitchDialog.js';
import { previewSwitch, sideEffects, switchLabel } from './switches.js';

const byLabel = (a: { label: string }, b: { label: string }) => a.label.localeCompare(b.label);
// AI Support isn't in the catalog: its switch shows or hides the DSP's button alone.
const aiSupport = { id: 'ai_support', label: 'AI Support', kind: 'support' } as const;
type Item = PageEntry | ConnectionEntry | typeof aiSupport;
// Each DSP gets only its own custom declarations alongside the shared catalog.
function grouped(
  catalog: readonly FeatureEntry[],
  custom: ReadonlySet<string>,
): [string, readonly Item[]][] {
  return [
    [
      'Features',
      [
        ...catalog.filter(
          (f): f is PageEntry => f.kind === 'page' && !f.mandatory && !custom.has(f.id),
        ),
        aiSupport,
      ].sort(byLabel),
    ],
    [
      'Custom',
      catalog.filter((f): f is PageEntry => f.kind === 'page' && custom.has(f.id)).sort(byLabel),
    ],
    [
      'Connections',
      catalog.filter((f): f is ConnectionEntry => f.kind === 'connection').sort(byLabel),
    ],
    [
      'Every DSP',
      catalog.filter((f): f is PageEntry => f.kind === 'page' && !!f.mandatory).sort(byLabel),
    ],
  ];
}
// Who among the DSP's people sees an optional feature or part, from the most to the fewest.
const sights: readonly (readonly [Visibility, string])[] = [
  ['shown', 'Everyone'],
  ['beta', 'Beta testers'],
  ['hidden', 'Hidden'],
];
const reach = (sight: Visibility) => sights.findIndex(([each]) => each === sight);
const seenBy: Record<Visibility, string> = {
  shown: 'shown to',
  beta: 'in beta at',
  hidden: 'hidden from',
};
const filters = [
  ['all', 'All'],
  ['on', 'On'],
  ['off', 'Off'],
  ['beta', 'Beta'],
  ['hidden', 'Hidden'],
] as const;
type Filter = (typeof filters)[number][0];
// What the tab shows of a DSP's features: what is switched on, and who sees what.
type Shown = { enabled: string[]; sights: Record<string, Visibility> };
type SwitchChanges = readonly { feature: string; enabled: boolean }[];
// What is switched on once `changes` are made.
function switchedAs(enabled: readonly string[], changes: SwitchChanges) {
  const on = new Set(enabled);
  for (const change of changes) {
    if (change.enabled) on.add(change.feature);
    else on.delete(change.feature);
  }
  return [...on];
}
// Names the chosen feature in the address, or none.
function nameInAddress(id: string) {
  const { feature = '', ...rest } = Object.fromEntries(hashQuery());
  if (feature !== id) replaceHashQuery(id ? { ...rest, feature: id } : rest);
}

// Every feature in a list beside the one chosen, searched by its name or its parts' and narrowed
// to what is on, off, in beta or hidden. The chosen one rides in the address; with none chosen,
// a wide screen shows the first listed and a phone the list alone.
//
// The chosen one's panel holds its switch and, for an optional feature, who sees it: the DSP's
// members, its beta testers alone, or none of them while it keeps running, as the Platform
// Owner view still shows it. Under it, its parts, each with its own switch and who sees it. A
// part reaches no more people than its feature: its choice shows the fewer of the two, and the
// wider choices wait, the part's own kept, until the feature reaches further again. A part
// can't be switched while its feature is off, and shows off; switching the feature back on
// brings each part back as it was. Then the connections it needs. A connection's panel lists
// what uses it. A change shows at once and saves behind it; a switch that takes other features
// with it asks first. AI Support's switch hides only its button from the DSP's members; the
// platform owner keeps it.
export function DspFeaturesTab({ dsp, changed }: { dsp: DspSummary; changed: () => void }) {
  const { data, error, refresh } = useDspFeatures(dsp.id);
  const custom = useMemo(() => new Set(dsp.customFeatures ?? []), [dsp.customFeatures]);
  const catalog = useMemo<readonly FeatureEntry[]>(
    () => [
      ...publicCatalog,
      ...((data?.custom ?? []).filter((entry) =>
        custom.has(entry.id),
      ) as unknown as FeatureEntry[]),
    ],
    [data?.custom, custom],
  );
  const groups = grouped(catalog, custom);
  const items = groups.flatMap(([, members]) => members);
  const suppliers = (feature: FeatureEntry) =>
    catalog.filter(
      (f): f is ConnectionEntry =>
        f.kind === 'connection' && feature.requires.some((c) => f.provides.includes(c)),
    );
  const [query, setQuery] = useUpdateState('dsp-feature-query', '');
  const [filter, setFilter] = useUpdateState<Filter>('dsp-feature-filter', 'all');
  const [chosen, setChosen] = useState(() => hashQuery().get('feature') ?? '');
  // A switch that takes other features with it, asked about before it is made.
  const [asking, setAsking] = useState<{ dsp: string; feature: FeatureEntry; on: boolean }>();
  // As read, with what was changed here over it: each change shows at once and saves behind the
  // ones before it, and once the last is saved the DSP's features are read again and the list's
  // counts follow. Before the features are read, what the DSP's summary says is on.
  const before = data ? undefined : dsp.features;
  const read = useMemo<Shown>(
    () => ({
      enabled: data
        ? data.features.filter((state) => state.enabled).map((state) => state.feature)
        : (before ?? []),
      sights: Object.fromEntries(
        (data?.features ?? []).map((state) => [state.feature, state.visibility]),
      ),
    }),
    [data, before],
  );
  const { shown, change } = useChanges(read, dsp.id, () => {
    refresh();
    changed();
  });
  // AI Support's button, as the DSP's profile says, with what was switched here over it.
  const support = useChanges(!dsp.profile.aiSupportHidden, dsp.id, changed);
  const supportShown = support.shown;
  // What is switched on, parts of a feature that is off included, so they come back as they were.
  const enabled = shown.enabled;
  const switched = (id: string) => enabled.includes(id);
  const stored = (id: string): Visibility => shown.sights[id] ?? 'shown';
  const visibility = (feature: FeatureEntry) => stored(feature.id);
  // Who sees it: a part no more people than its feature, whatever its own choice.
  const reaches = (feature: FeatureEntry) =>
    feature.kind === 'sub'
      ? sights[Math.max(reach(stored(feature.page)), reach(stored(feature.id)))]![0]
      : stored(feature.id);
  // What the DSP has: a part only while its feature is on too.
  const has = (feature: FeatureEntry) =>
    switched(feature.id) && (feature.kind !== 'sub' || switched(feature.page));
  const isOn = (item: Item) => (item.kind === 'support' ? supportShown : has(item));
  // Whether `item` is among what `to` shows: in beta or hidden, a feature that is on, by itself
  // or by one of its parts that is.
  const passes = (item: Item, to: Filter) => {
    if (to === 'all') return true;
    if (to === 'on' || to === 'off') return isOn(item) === (to === 'on');
    return (
      item.kind === 'page' &&
      has(item) &&
      [item, ...subsOf(item.id, catalog).filter(has)].some((f) => reaches(f) === to)
    );
  };
  const named = (item: Item) => {
    const words = query.trim().toLowerCase();
    return [item, ...subsOf(item.id, catalog)].some((f) => f.label.toLowerCase().includes(words));
  };
  const listed = groups
    .map(([name, members]) => [name, members.filter((m) => named(m) && passes(m, filter))] as const)
    .filter(([, members]) => members.length);
  const explicit = items.find((item) => item.id === chosen);
  const selected = explicit ?? listed[0]?.[1][0];
  // The address names the chosen one, written only as the address or the choice changes, so a
  // Back straight after never finds it rewritten. An address followed, by Back or a link, opens
  // the feature it names; one naming none, as opening another DSP does, keeps the one chosen.
  const chosenNow = useRef(chosen);
  chosenNow.current = chosen;
  useEffect(() => {
    const followed = () => {
      if (parseHash(location.hash).page !== 'dsps') return;
      const feature = hashQuery().get('feature');
      if (feature) setChosen(feature);
      else if (hashQuery().has('dsp')) nameInAddress(chosenNow.current);
    };
    addEventListener('hashchange', followed);
    return () => removeEventListener('hashchange', followed);
  }, []);
  const choose = (id: string) => {
    setChosen(id);
    nameInAddress(id);
  };
  // A save, saying what it did once it has.
  const saving = useAction((work: () => Promise<unknown>, _done: string) => work(), {
    success: (_work, done) => done,
  });
  const switchTo = (feature: FeatureEntry, on: boolean, changes: SwitchChanges) =>
    change(
      (value) => ({ ...value, enabled: switchedAs(value.enabled, changes) }),
      () =>
        saving.run(
          () => setDspFeature(dsp.id, feature.id, on),
          `${switchLabel(feature)} switched ${on ? 'on' : 'off'}`,
        ),
    );
  const toggle = (feature: FeatureEntry, on: boolean) => {
    const preview = previewSwitch(enabled, feature.id, on, catalog);
    // Alone, or already as asked: no question to ask.
    if (preview && !sideEffects(feature, preview, catalog).length) {
      if (preview.length) switchTo(feature, on, preview);
      return;
    }
    setAsking({ dsp: dsp.id, feature, on });
  };
  const see = (feature: FeatureEntry, to: Visibility) =>
    change(
      (value) => ({ ...value, sights: { ...value.sights, [feature.id]: to } }),
      () =>
        saving.run(
          () => setDspVisibility(dsp.id, feature.id, to),
          `${switchLabel(feature)} ${seenBy[to]} ${dsp.name}`,
        ),
    );
  const showSupport = (on: boolean) =>
    support.change(
      () => on,
      () =>
        saving.run(
          () => showDspAiSupport(dsp.id, on),
          `AI Support ${on ? 'shown to' : 'hidden from'} ${dsp.name}`,
        ),
    );
  const control = (feature: FeatureEntry, locked = false) => (
    <input
      type="checkbox"
      role="switch"
      aria-label={switchLabel(feature)}
      title={feature.mandatory ? 'Every DSP has it' : undefined}
      checked={has(feature)}
      disabled={!data || locked || Boolean(feature.mandatory)}
      onChange={(event) => toggle(feature, event.target.checked)}
    />
  );
  const connected = (connection: ConnectionEntry) => (
    <Badge value={dsp.connections[connection.id] ?? 'not_connected'} />
  );
  // What a row of the list says of its feature beside its name.
  const note = (item: Item) => {
    if (item.kind === 'support') return supportShown ? 'On' : 'Off';
    if (item.mandatory) return '';
    if (!has(item)) return 'Off';
    if (item.kind === 'connection') return connected(item);
    return { shown: 'On', beta: 'Beta', hidden: 'Hidden' }[visibility(item)];
  };
  // A row that opens another feature's panel.
  const link = (id: string, label: string, status: ReactNode) => (
    <li key={id}>
      <button className="dsp-feature-row" onClick={() => choose(id)}>
        <strong>{label}</strong>
        {status}
        <ChevronRight size={16} aria-hidden="true" />
      </button>
    </li>
  );
  // A feature's parts, each with its switch and who sees it, which reaches no further than the
  // feature does.
  const parts = (page: PageEntry) => {
    const widest = reach(visibility(page));
    return (
      <ul className="dsp-feature-rows" aria-label={`${page.label}'s parts`}>
        {subsOf(page.id, catalog).map((part) => {
          const sight = reaches(part);
          const needs = suppliers(part).map((f) => f.label);
          return (
            <li key={part.id} className={`dsp-feature-row ${has(part) ? '' : 'off'}`}>
              {control(part, !has(page))}
              <strong>{part.label}</strong>
              {needs.length > 0 && <small>Needs {needs.join(' or ')}</small>}
              {!part.mandatory && (
                <select
                  aria-label={`Who sees ${switchLabel(part)}`}
                  data-sight={sight}
                  value={sight}
                  disabled={!data || widest === sights.length - 1}
                  onChange={(event) => see(part, event.target.value as Visibility)}
                >
                  {sights.map(([to, label], index) => (
                    <option key={to} value={to} disabled={index < widest}>
                      {label}
                    </option>
                  ))}
                </select>
              )}
            </li>
          );
        })}
      </ul>
    );
  };
  // The chosen one's panel, under its name and switch.
  const details = (item: Item) => {
    if (item.kind === 'support') return null;
    if (item.kind === 'connection')
      return (
        <>
          <h4 className="dsp-group">Used by</h4>
          <ul className="dsp-feature-rows">
            {catalog
              .filter((f) => f.kind !== 'connection' && suppliers(f).includes(item))
              .map((f) =>
                link(
                  f.kind === 'sub' ? f.page : f.id,
                  featureLabel(f.id, catalog),
                  <small>{has(f) ? 'On' : 'Off'}</small>,
                ),
              )}
          </ul>
        </>
      );
    const needs = suppliers(item);
    return (
      <>
        {!item.mandatory && (
          <>
            <h4 className="dsp-group">Who sees it</h4>
            <div
              className="dsp-sight"
              role="radiogroup"
              aria-label={`Who sees ${switchLabel(item)}`}
            >
              {sights.map(([to, label]) => (
                <button
                  key={to}
                  data-sight={to}
                  role="radio"
                  aria-checked={visibility(item) === to}
                  disabled={!data}
                  onClick={() => {
                    if (visibility(item) !== to) see(item, to);
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
          </>
        )}
        {subsOf(item.id, catalog).length > 0 && (
          <>
            <h4 className="dsp-group">Parts</h4>
            {parts(item)}
          </>
        )}
        {needs.length > 0 && (
          <>
            <h4 className="dsp-group">Needs</h4>
            <ul className="dsp-feature-rows">
              {needs.map((connection) =>
                link(
                  connection.id,
                  connection.label,
                  has(connection) ? connected(connection) : <small>Off</small>,
                ),
              )}
            </ul>
          </>
        )}
      </>
    );
  };
  return (
    <div className="dsp-feature-tab">
      <ErrorBox message={error} />
      <div className={`dsp-feature-browser ${explicit ? 'chosen' : ''}`}>
        <nav className="dsp-feature-list-pane" aria-label="Features">
          <SearchInput
            label="Search features"
            placeholder="Search features"
            value={query}
            onChange={setQuery}
          />
          <div className="dsp-feature-filters" role="group" aria-label="Show">
            {filters.map(([to, label]) => (
              <button key={to} aria-pressed={filter === to} onClick={() => setFilter(to)}>
                {label} <i>{items.filter((item) => passes(item, to)).length}</i>
              </button>
            ))}
          </div>
          {listed.length ? (
            listed.map(([name, members]) => (
              <div key={name} className="dsp-feature-items" role="group" aria-label={name}>
                <h3 className="dsp-group">{name}</h3>
                {members.map((item) => (
                  <button
                    key={item.id}
                    className={`dsp-feature-item ${isOn(item) ? '' : 'off'}`}
                    aria-current={item === selected ? 'true' : undefined}
                    onClick={() => choose(item.id)}
                  >
                    <i aria-hidden="true" />
                    <span>{item.label}</span>
                    <small
                      data-sight={item.kind === 'page' && has(item) ? visibility(item) : undefined}
                    >
                      {note(item)}
                    </small>
                  </button>
                ))}
              </div>
            ))
          ) : (
            <Empty title="No features found" />
          )}
        </nav>
        {selected && (
          <section className="dsp-feature-panel" aria-label={selected.label}>
            <button className="text-button dsp-features-back" onClick={() => choose('')}>
              <ChevronLeft size={16} />
              All features
            </button>
            <div className="dsp-feature-head">
              <h3>{selected.label}</h3>
              {selected.kind === 'connection' && has(selected) && connected(selected)}
              <label className="dsp-feature-switch">
                {isOn(selected) ? 'On' : 'Off'}
                {selected.kind === 'support' ? (
                  <input
                    type="checkbox"
                    role="switch"
                    aria-label="AI Support"
                    title="The AI Support button in the DSP's account menu"
                    checked={supportShown}
                    onChange={(event) => showSupport(event.target.checked)}
                  />
                ) : (
                  control(selected)
                )}
              </label>
            </div>
            {details(selected)}
          </section>
        )}
      </div>
      {asking?.dsp === dsp.id && data && (
        <FeatureSwitchDialog
          feature={asking.feature}
          on={asking.on}
          enabled={enabled}
          catalog={catalog}
          onClose={() => setAsking(undefined)}
          onConfirm={(changes) => {
            setAsking(undefined);
            switchTo(asking.feature, asking.on, changes);
          }}
        />
      )}
    </div>
  );
}
