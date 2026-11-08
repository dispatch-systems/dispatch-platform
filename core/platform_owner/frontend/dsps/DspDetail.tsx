import { useEffect } from 'react';
import { ChevronLeft, Ellipsis, Eye } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import { useUpdateState } from '../../../shell/frontend/runtime/navigation-state.js';
import { connectionFeatures } from '../../../shell/frontend/runtime/features.js';
import {
  hashQuery,
  parseHash,
  replaceHashQuery,
} from '../../../shell/frontend/runtime/navigation.js';
import { dspAddress } from '../../../shell/frontend/runtime/site.js';
import { Badge, DspAvatar, Popover, Tabs } from '../../../shell/frontend/ui/index.js';
import { deviceTimezone, time } from '../../../shell/frontend/lib/format.js';
import { DspBackupsTab } from './DspBackupsTab.js';
import { DspBetaTab } from './DspBetaTab.js';
import { DspFeaturesTab } from './DspFeaturesTab.js';
import { dspState, ownerText, stateLabels } from './status.js';

export type DspAction = 'view' | 'code' | 'enable' | 'retry' | 'disable' | 'delete';

// The chosen DSP across the page, a way back to them all and to another beside its name: its facts,
// its features, its beta testers, its backups and what the platform may do to it.
export function DspDetail({
  dsp,
  back,
  act,
  changed,
  switcher,
}: {
  dsp: DspSummary;
  back: () => void;
  act: (action: DspAction) => void;
  /** A feature was switched; the list's counts follow. */
  changed: () => void;
  /** Opens another DSP in its place. */
  switcher: React.ReactNode;
}) {
  // A feature named in the address opens the Features tab: a link to it, a reload or Back.
  const [tab, setTab] = useUpdateState('dsp-tab', () =>
    hashQuery().has('feature') ? 'features' : 'details',
  );
  useEffect(() => {
    const followed = () => {
      if (hashQuery().has('feature') && parseHash(location.hash).page === 'dsps')
        setTab('features');
    };
    addEventListener('hashchange', followed);
    return () => removeEventListener('hashchange', followed);
  }, [setTab]);
  // Leaving the Features tab leaves its feature out of the address.
  const openTab = (to: string) => {
    setTab(to);
    const { feature, ...rest } = Object.fromEntries(hashQuery());
    if (feature && to !== 'features') replaceHashQuery(rest);
  };
  const state = dspState(dsp);
  const viewable = dsp.status === 'active';
  const actions: [DspAction, string, boolean][] = [
    ['view', 'View', viewable],
    [
      'code',
      dsp.code ? 'Change short code' : 'Set short code',
      ['active', 'suspended'].includes(dsp.status),
    ],
    ['retry', 'Retry', dsp.status === 'failed'],
    ['enable', 'Enable DSP', dsp.status === 'suspended' && !dsp.permanent],
    ['disable', 'Disable DSP', dsp.status === 'active' && !dsp.permanent],
    ['delete', 'Delete DSP', dsp.status === 'suspended' && !dsp.permanent],
  ];
  // The View button stands on its own; the menu holds everything else.
  const menu = actions.filter(([action, , shown]) => shown && action !== 'view');
  const zone = deviceTimezone();
  const connections = connectionFeatures(dsp.features);
  const facts: [string, React.ReactNode][] = [
    ['Owner', ownerText(dsp)],
    [
      'Address',
      dsp.code ? (
        <a href={dspAddress(dsp.code)} target="_blank" rel="noopener noreferrer">
          {new URL(dspAddress(dsp.code)).host}
        </a>
      ) : (
        'None until it has a short code'
      ),
    ],
    ['Members', String(dsp.members)],
    ['Created', time(dsp.createdAt, zone)],
    ['Station', dsp.profile.stationCode || '—'],
    ['Timezone', dsp.timezone],
    ['Setup', dsp.profile.setupRequired ? 'Incomplete' : 'Complete'],
    ...connections.map((feature): [string, React.ReactNode] => [
      feature.label,
      <Badge key={feature.id} value={dsp.connections[feature.id] ?? 'not_connected'} />,
    ]),
    ['Last collection', time(dsp.lastCollection, zone)],
    ['Next collection', time(dsp.nextCollection, zone, 'Nothing scheduled')],
  ];
  return (
    <section className="dsps-detail" aria-label={dsp.name}>
      <button className="text-button dsp-back" onClick={back}>
        <ChevronLeft size={16} />
        All DSPs
      </button>
      <div className="dsp-detail-head">
        <DspAvatar name={dsp.name} />
        <div className="dsp-detail-copy">
          <div className="dsp-detail-title">
            <h2>
              {dsp.name}
              <Badge value={state}>{stateLabels[state]}</Badge>
            </h2>
            {switcher}
          </div>
          <small>{[dsp.profile.abbreviation, dsp.ownerEmail].filter(Boolean).join(' · ')}</small>
        </div>
        <div className="dsp-detail-actions">
          <button className="primary" disabled={!viewable} onClick={() => act('view')}>
            <Eye size={16} />
            View
          </button>
          {menu.length > 0 && (
            <Popover
              className="row-menu"
              label={`Actions for ${dsp.name}`}
              trigger={<Ellipsis size={18} />}
              anchored
            >
              {menu.map(([action, label]) => (
                <button
                  key={action}
                  className={action === 'disable' || action === 'delete' ? 'danger' : undefined}
                  onClick={() => act(action)}
                >
                  {label}
                </button>
              ))}
            </Popover>
          )}
        </div>
      </div>
      <Tabs
        value={tab}
        onChange={openTab}
        items={[
          ['details', 'Details'],
          ['features', 'Features'],
          ['beta', 'Beta testers'],
          ['backups', 'Backups'],
        ]}
        label="DSP"
      />
      {tab === 'features' ? (
        <DspFeaturesTab dsp={dsp} changed={changed} />
      ) : tab === 'beta' ? (
        <DspBetaTab dsp={dsp} />
      ) : tab === 'backups' ? (
        <DspBackupsTab dsp={dsp} />
      ) : (
        <dl className="dsp-facts">
          {facts.map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}
