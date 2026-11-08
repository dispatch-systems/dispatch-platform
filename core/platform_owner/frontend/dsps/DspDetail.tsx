import { useEffect } from 'react';
import { ChevronLeft, Ellipsis, ExternalLink, Eye } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import { useUpdateState } from '../../../shell/frontend/runtime/navigation-state.js';
import {
  hashQuery,
  parseHash,
  replaceHashQuery,
} from '../../../shell/frontend/runtime/navigation.js';
import { dspAddress } from '../../../shell/frontend/runtime/site.js';
import { Badge, DspAvatar, Popover, Tabs } from '../../../shell/frontend/ui/index.js';
import { DspBackupsTab } from './DspBackupsTab.js';
import { DspBetaTab } from './DspBetaTab.js';
import { DspFeaturesTab } from './DspFeaturesTab.js';
import { dspState, stateLabels } from './status.js';

export type DspAction = 'view' | 'code' | 'enable' | 'retry' | 'disable' | 'delete';

// The chosen DSP across the page, a way back to them all and to another beside its name: who owns it
// and its address, its features, its beta testers, its backups and what the platform may do to it.
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
  // A DSP opens on its features; a feature named in the address opens them too, over the tab last
  // open.
  const [saved, setTab] = useUpdateState('dsp-tab', 'features');
  // A tab saved before the Details tab went opens Features.
  const tab = saved === 'beta' || saved === 'backups' ? saved : 'features';
  useEffect(() => {
    const followed = () => {
      if (hashQuery().has('feature') && parseHash(location.hash).page === 'dsps')
        setTab('features');
    };
    followed();
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
          <small className="dsp-detail-meta">
            {dsp.profile.abbreviation && <span>{dsp.profile.abbreviation}</span>}
            {dsp.ownerEmail && <span className="dsp-detail-owner">{dsp.ownerEmail}</span>}
            {dsp.code && (
              <a href={dspAddress(dsp.code)} target="_blank" rel="noopener noreferrer">
                {new URL(dspAddress(dsp.code)).host}
                <ExternalLink size={12} />
              </a>
            )}
          </small>
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
          ['features', 'Features'],
          ['beta', 'Beta testers'],
          ['backups', 'Backups'],
        ]}
        label="DSP"
      />
      {tab === 'beta' ? (
        <DspBetaTab dsp={dsp} />
      ) : tab === 'backups' ? (
        <DspBackupsTab dsp={dsp} />
      ) : (
        <DspFeaturesTab dsp={dsp} changed={changed} />
      )}
    </section>
  );
}
