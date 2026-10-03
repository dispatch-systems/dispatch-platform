import { useState } from 'react';
import type { AgentActivity } from '../../../../shared/contracts/index.js';
import {
  readAgentActivity,
  useAgentActivity,
  useAgentKeys,
  type AgentActivityFilter,
} from '../../app/endpoints.js';
import { useAction } from '../../app/useAction.js';
import { activityNote, surfaceOf } from '../../lib/agents.js';
import { deviceTimezone, duration, timeWithSeconds } from '../../lib/format.js';
import {
  Badge,
  DataState,
  DataTable,
  Empty,
  ErrorBox,
  useDataTable,
  type TableColumn,
} from '../../ui/index.js';

const columns = (timeZone: string): TableColumn<AgentActivity>[] => [
  {
    id: 'at',
    header: 'Time',
    cell: (call) => <time dateTime={call.at}>{timeWithSeconds(call.at, timeZone)}</time>,
  },
  {
    id: 'connection',
    header: 'Connection',
    className: 'agents-wrap',
    cell: (call) => (
      <span className="agents-caller">
        <bdi>{call.key.name}</bdi>
        <Badge value={call.key.kind}>{call.key.kind === 'app' ? 'App' : 'Key'}</Badge>
      </span>
    ),
  },
  {
    id: 'surface',
    header: 'Tool or endpoint',
    cell: (call) => {
      const { via, name } = surfaceOf(call.surface);
      return (
        <span className="agents-surface">
          <code className="agents-mono">{name}</code>
          {via && <small>{via}</small>}
        </span>
      );
    },
  },
  { id: 'dsp', header: 'DSP', cell: (call) => call.dsp?.name ?? '—' },
  {
    id: 'outcome',
    header: 'Outcome',
    cell: (call) => (
      <span className="agents-outcome">
        {call.outcome === 'ok' ? (
          <Badge value="succeeded">OK</Badge>
        ) : (
          <Badge value="failed">{call.outcome}</Badge>
        )}
        {/* It read a feature the DSP has switched off. */}
        {call.bypassed && <span className="agents-bypassed">Bypassed</span>}
      </span>
    ),
  },
  {
    id: 'ms',
    header: 'Duration',
    className: 'cell-end',
    headerClassName: 'cell-end',
    cell: (call) => duration(call.ms),
  },
];

/** Every call a key or connected app made, newest first, a page at a time. */
export function ActivityTab() {
  const timeZone = deviceTimezone();
  const agents = useAgentKeys();
  const [filter, setFilter] = useState<AgentActivityFilter>({ key: '', outcome: '' });
  const first = useAgentActivity(filter);
  // Pages read past the first, for the first page they follow and its filter.
  const [more, setMore] = useState<{ after: string; rows: AgentActivity[]; next: string | null }>();
  const after = `${JSON.stringify(filter)} ${first.data?.next}`;
  const extra = more?.after === after ? more : undefined;
  const next = extra ? extra.next : first.data?.next;
  const load = useAction(
    async (cursor: string) => {
      const page = await readAgentActivity(filter, cursor);
      setMore({ after, rows: [...(extra?.rows ?? []), ...page.rows], next: page.next });
    },
    { inline: true },
  );
  // Another filter's answer holds the table in place, marked busy, until this one's arrives.
  const page = first.data ?? first.stale;
  const rows = page ? [...page.rows, ...(extra?.rows ?? [])] : [];
  const table = useDataTable({
    columns: columns(timeZone),
    rows,
    rowId: (call, index) => `${call.at} ${call.key.id} ${index}`,
  });
  const set = (change: Partial<AgentActivityFilter>) =>
    setFilter((current) => ({ ...current, ...change }));
  const all = agents.data?.keys ?? [];
  const groups = [
    ['Keys', all.filter((key) => key.kind === 'key')],
    ['Connected apps', all.filter((key) => key.kind === 'app')],
  ] as const;
  return (
    <div className="agents" aria-busy={!first.data && Boolean(page)}>
      <div className="table-toolbar agents-filters">
        <select
          aria-label="Connection"
          value={filter.key}
          onChange={(event) => set({ key: event.target.value })}
        >
          <option value="">All connections</option>
          {groups.map(
            ([label, list]) =>
              list.length > 0 && (
                <optgroup key={label} label={label}>
                  {list.map((key) => (
                    <option key={key.id} value={key.id}>
                      {key.name}
                    </option>
                  ))}
                </optgroup>
              ),
          )}
        </select>
        <div className="agents-segmented" role="group" aria-label="Outcome">
          {(
            [
              ['', 'All'],
              ['refused', 'Refused'],
            ] as const
          ).map(([outcome, label]) => (
            <button
              key={label}
              type="button"
              aria-pressed={filter.outcome === outcome}
              onClick={() => set({ outcome })}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <DataState data={page} error={first.error} retry={first.refresh}>
        {() =>
          rows.length === 0 ? (
            <Empty
              title={filter.key || filter.outcome ? 'No matching calls' : 'No agent calls yet'}
            />
          ) : (
            <>
              <div className="table-wrap">
                <DataTable
                  table={table}
                  className="agents-calls"
                  label="Agent calls"
                  rowClassName={(call) => (call.outcome === 'capped' ? 'agents-note' : undefined)}
                  renderNote={(call) => activityNote(call)}
                />
              </div>
              <ErrorBox message={load.error} />
              {next && (
                <div className="agents-footer">
                  <button type="button" disabled={load.busy} onClick={() => void load.run(next)}>
                    Load more
                  </button>
                </div>
              )}
            </>
          )
        }
      </DataState>
    </div>
  );
}
