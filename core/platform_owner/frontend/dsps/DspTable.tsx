import { useSyncExternalStore } from 'react';
import { RefreshCw } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import { featureCatalog } from '../../../shell/frontend/runtime/features.js';
import { calendarDay, deviceTimezone, time } from '../../../shell/frontend/lib/format.js';
import {
  Badge,
  DataTable,
  DspAvatar,
  Empty,
  SearchInput,
  useDataTable,
  type TableColumn,
} from '../../../shell/frontend/ui/index.js';
import { dspState, dspStates, ownerText, stateLabels } from './status.js';

// What a DSP may have switched on or off: what every DSP has isn't counted.
const optional = featureCatalog.filter((f) => f.kind === 'page' && !f.mandatory);
const featuresOn = (dsp: DspSummary) => optional.filter((f) => dsp.features.includes(f.id)).length;
// A phone's width, where a row is each DSP's name, owner and state alone.
const narrow = '(max-width: 760px)';
const watchNarrow = (changed: () => void) => {
  const query = matchMedia(narrow);
  query.addEventListener('change', changed);
  return () => query.removeEventListener('change', changed);
};
const useNarrow = () => useSyncExternalStore(watchNarrow, () => matchMedia(narrow).matches);
const status = (dsp: DspSummary) => (
  <Badge value={dspState(dsp)}>{stateLabels[dspState(dsp)]}</Badge>
);
// An online DSP whose owner hasn't finished setting it up.
const setupIncomplete = (dsp: DspSummary) =>
  dspState(dsp) === 'online' &&
  dsp.profile.setupRequired && <small className="dsp-setup">Setup incomplete</small>;

// Every DSP, a row each, across the page: its name opens it. On a phone, a row is its name, owner
// and state, and whether its setup is unfinished, and opens it.
export function DspTable({
  dsps,
  query,
  setQuery,
  onSelect,
  refresh,
}: {
  dsps: DspSummary[];
  query: string;
  setQuery: (value: string) => void;
  onSelect: (dsp: DspSummary) => void;
  refresh: () => void;
}) {
  const zone = deviceTimezone();
  const phone = useNarrow();
  const columns: TableColumn<DspSummary>[] = [
    {
      id: 'dsp',
      header: 'DSP',
      rowHeader: true,
      sortable: true,
      value: (dsp) => dsp.name,
      cell: (dsp) => (
        <button className="dsp-name" onClick={() => onSelect(dsp)}>
          <DspAvatar name={dsp.name} />
          <span className="dsp-name-copy">
            <strong>{dsp.name}</strong>
            {dsp.profile.abbreviation && <small>{dsp.profile.abbreviation}</small>}
          </span>
        </button>
      ),
    },
    {
      id: 'status',
      header: 'Status',
      sortable: true,
      value: (dsp) => dspStates.indexOf(dspState(dsp)),
      cell: (dsp) => (
        <span className="dsp-cell-lines">
          {status(dsp)}
          {setupIncomplete(dsp)}
        </span>
      ),
    },
    { id: 'owner', header: 'Owner', value: ownerText, cell: ownerText },
    {
      id: 'station',
      header: 'Station',
      sortable: true,
      value: (dsp) => dsp.profile.stationCode,
      cell: (dsp) => dsp.profile.stationCode || '—',
    },
    {
      id: 'members',
      header: 'Members',
      sortable: true,
      value: (dsp) => dsp.members,
      cell: (dsp) => dsp.members,
    },
    {
      id: 'features',
      header: 'Features on',
      sortable: true,
      value: featuresOn,
      cell: (dsp) => `${featuresOn(dsp)} of ${optional.length}`,
    },
    {
      id: 'created',
      header: 'Created',
      sortable: true,
      value: (dsp) => dsp.createdAt,
      cell: (dsp) => calendarDay(dsp.createdAt),
    },
    {
      id: 'collection',
      header: 'Last collection',
      sortable: true,
      value: (dsp) => dsp.lastCollection ?? '',
      cell: (dsp) => (
        <span className="dsp-cell-lines">
          {time(dsp.lastCollection, zone)}
          {dsp.nextCollection && <small>Next: {time(dsp.nextCollection, zone)}</small>}
        </span>
      ),
    },
  ];
  const table = useDataTable({ columns, rows: dsps, rowId: (dsp) => dsp.id });
  return (
    <section className="dsps-table" aria-label="DSPs">
      <div className="dsps-list-tools">
        <SearchInput
          label="Search DSPs"
          placeholder="Search DSPs or owner email"
          value={query}
          onChange={setQuery}
        />
        <button className="icon-button" aria-label="Refresh DSPs" onClick={refresh}>
          <RefreshCw size={16} />
        </button>
      </div>
      {!dsps.length ? (
        <Empty title="No DSPs found">Try another search or create your first DSP.</Empty>
      ) : phone ? (
        <ul className="dsp-rows">
          {dsps.map((dsp) => (
            <li key={dsp.id}>
              <button className="dsp-name" onClick={() => onSelect(dsp)}>
                <DspAvatar name={dsp.name} />
                <span className="dsp-name-copy">
                  <strong>{dsp.name}</strong>
                  <small>{ownerText(dsp)}</small>
                  {setupIncomplete(dsp)}
                </span>
                {status(dsp)}
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <div className="table-wrap">
          <DataTable table={table} label="DSPs" />
        </div>
      )}
    </section>
  );
}
