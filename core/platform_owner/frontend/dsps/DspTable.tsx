import { RefreshCw } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import { featureCatalog } from '../../../shell/frontend/runtime/features.js';
import { deviceTimezone, time } from '../../../shell/frontend/lib/format.js';
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

// Every DSP, a row each, across the page: its name opens it.
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
          <span>
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
      cell: (dsp) => <Badge value={dspState(dsp)}>{stateLabels[dspState(dsp)]}</Badge>,
    },
    { id: 'owner', header: 'Owner', value: ownerText, cell: ownerText },
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
      id: 'collection',
      header: 'Last collection',
      sortable: true,
      value: (dsp) => dsp.lastCollection ?? '',
      cell: (dsp) => time(dsp.lastCollection, zone),
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
      {dsps.length ? (
        <DataTable table={table} label="DSPs" />
      ) : (
        <Empty title="No DSPs found">Try another search or create your first DSP.</Empty>
      )}
    </section>
  );
}
