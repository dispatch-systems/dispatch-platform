import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import type { DspFeatureReport, DspFeatures, Visibility } from '../../api/index.js';
import {
  setDspFeature,
  setDspVisibility,
  showDspAiSupport,
  useDspFeatures,
} from '../../api/client.js';
import {
  featureCatalog,
  featureLabel,
  subsOf,
  type ConnectionEntry,
  type FeatureEntry,
  type PageEntry,
} from '../../../shell/frontend/runtime/features.js';
import { useUpdateState } from '../../../shell/frontend/runtime/navigation-state.js';
import {
  hashQuery,
  parseHash,
  replaceHashQuery,
} from '../../../shell/frontend/runtime/navigation.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { Badge, Empty, ErrorBox, SearchInput } from '../../../shell/frontend/ui/index.js';
import { FeatureSwitchDialog } from './FeatureSwitchDialog.js';
import { previewSwitch, sideEffects, switchLabel } from './switches.js';

const byLabel = (a: { label: string }, b: { label: string }) => a.label.localeCompare(b.label);
// AI Support isn't in the catalog: its switch shows or hides the DSP's button alone.
const aiSupport = { id: 'ai_support', label: 'AI Support', kind: 'support' } as const;
type Item = PageEntry | ConnectionEntry | typeof aiSupport;
// The list's groups, each alphabetical: the optional features with AI Support, the connections,
// then what every DSP has.
const groups: [string, readonly Item[]][] = [
  [
    'Features',
    [
      ...featureCatalog.filter((f): f is PageEntry => f.kind === 'page' && !f.mandatory),
      aiSupport,
    ].sort(byLabel),
  ],
  [
    'Connections',
    featureCatalog.filter((f): f is ConnectionEntry => f.kind === 'connection').sort(byLabel),
  ],
  [
    'Every DSP',
    featureCatalog.filter((f): f is PageEntry => f.kind === 'page' && !!f.mandatory).sort(byLabel),
  ],
];
const items = groups.flatMap(([, members]) => members);
// The connections that supply what a feature or part requires.
const suppliers = (feature: FeatureEntry) =>
  featureCatalog.filter(
    (f): f is ConnectionEntry =>
      f.kind === 'connection' && feature.requires.some((c) => f.provides.includes(c)),
  );
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
// what uses it. A switch acts at once; one that takes other features with it asks first. AI
// Support's switch hides only its button from the DSP's members; the platform owner keeps it.
export function DspFeaturesTab({ dsp, changed }: { dsp: DspSummary; changed: () => void }) {
  const { data, error, refresh } = useDspFeatures(dsp.id);
  const [query, setQuery] = useUpdateState('dsp-feature-query', '');
  const [filter, setFilter] = useUpdateState<Filter>('dsp-feature-filter', 'all');
  const [chosen, setChosen] = useState(() => hashQuery().get('feature') ?? '');
  const [pending, setPending] = useState<{ feature: FeatureEntry; on: boolean }>();
  // As the switch last answered, until the DSP's profile is read again.
  const [support, setSupport] = useState<{ dsp: string; shown: boolean }>();
  const supportShown = support?.dsp === dsp.id ? support.shown : !dsp.profile.aiSupportHidden;
  // What the last switch answered is on, until the DSP's features are read again: a switch made
  // before that reading lands goes by the answer, not by the reading before it.
  const [answered, setAnswered] = useState<{ read?: DspFeatureReport; features: string[] }>();
  const latest = useRef(data);
  latest.current = data;
  // What is switched on, parts of a feature that is off included, so they come back as they were.
  const enabled =
    answered && answered.read === data
      ? answered.features
      : data
        ? data.features.filter((state) => state.enabled).map((state) => state.feature)
        : dsp.features;
  const switched = (id: string) => enabled.includes(id);
  const stored = (id: string): Visibility =>
    data?.features.find((state) => state.feature === id)?.visibility ?? 'shown';
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
      [item, ...subsOf(item.id).filter(has)].some((f) => reaches(f) === to)
    );
  };
  const named = (item: Item) => {
    const words = query.trim().toLowerCase();
    return [item, ...subsOf(item.id)].some((f) => f.label.toLowerCase().includes(words));
  };
  const listed = groups
    .map(([name, members]) => [name, members.filter((m) => named(m) && passes(m, filter))] as const)
    .filter(([, members]) => members.length);
  const explicit = items.find((item) => item.id === chosen);
  const selected = explicit ?? listed[0]?.[1][0];
  // An address followed, by Back or a link, opens the feature it names; one naming none, as
  // choosing another DSP does, keeps the one chosen.
  useEffect(() => {
    const followed = () => {
      const feature = hashQuery().get('feature');
      if (feature && parseHash(location.hash).page === 'dsps') setChosen(feature);
    };
    addEventListener('hashchange', followed);
    return () => removeEventListener('hashchange', followed);
  }, []);
  // Choosing one names it in the address at once, so Back straight after finds it there; and
  // choosing another DSP, which rewrites the address, names it again.
  const choose = (id: string) => {
    setChosen(id);
    nameInAddress(id);
  };
  useEffect(() => nameInAddress(chosen), [chosen, dsp.id]);
  const done = () => {
    refresh();
    changed();
  };
  const switchedTo = (answer: DspFeatures) => {
    setAnswered({ read: latest.current, features: answer.features });
    done();
  };
  const direct = useAction(
    async (feature: FeatureEntry, on: boolean) => {
      switchedTo(await setDspFeature(dsp.id, feature.id, on));
    },
    { success: (feature, on) => `${switchLabel(feature)} switched ${on ? 'on' : 'off'}` },
  );
  const seeing = useAction(
    async (feature: FeatureEntry, to: Visibility) => {
      await setDspVisibility(dsp.id, feature.id, to);
      done();
    },
    { success: (feature, to) => `${switchLabel(feature)} ${seenBy[to]} ${dsp.name}` },
  );
  const supporting = useAction(
    async (shown: boolean) => {
      const profile = await showDspAiSupport(dsp.id, shown);
      setSupport({ dsp: dsp.id, shown: !profile.aiSupportHidden });
      changed();
    },
    { success: (shown) => `AI Support ${shown ? 'shown to' : 'hidden from'} ${dsp.name}` },
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
        {subsOf(page.id).map((part) => {
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
                  disabled={!data || seeing.busy || widest === sights.length - 1}
                  onChange={(event) => void seeing.run(part, event.target.value as Visibility)}
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
            {featureCatalog
              .filter((f) => f.kind !== 'connection' && suppliers(f).includes(item))
              .map((f) =>
                link(
                  f.kind === 'sub' ? f.page : f.id,
                  featureLabel(f.id),
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
                  disabled={!data || seeing.busy}
                  onClick={() => {
                    if (visibility(item) !== to) void seeing.run(item, to);
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
          </>
        )}
        {subsOf(item.id).length > 0 && (
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
                    disabled={supporting.busy}
                    onChange={(event) => void supporting.run(event.target.checked)}
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
      {pending && data && (
        <FeatureSwitchDialog
          dsp={dsp}
          feature={pending.feature}
          on={pending.on}
          enabled={enabled}
          onClose={() => setPending(undefined)}
          onDone={(answer) => {
            setPending(undefined);
            switchedTo(answer);
          }}
        />
      )}
    </div>
  );
}
