import { PaycomDateControls } from '../../DateControls.js';
import { localDate } from '../../../../../core/shell/frontend/lib/day.js';
import { useEffect, useMemo, useState } from 'react';
import { Download, Globe, Info } from 'lucide-react';
import type { DailyTimecard, PaycomPreferences } from '../../../api/index.js';
import {
  dailyPageUrl,
  exportDailyTimecards,
  useDailyTimecards,
  useDailyPunches,
} from '../../../api/client.js';
import { useTableState } from '../../../../../core/shell/frontend/runtime/useTableState.js';
import {
  DataState,
  DataTable,
  downloadTable,
  Empty,
  ErrorBox,
  Modal,
  TablePagination,
  useDataTable,
  type TableColumn,
} from '../../../../../core/shell/frontend/ui/index.js';
import { time } from '../../../../../core/shell/frontend/lib/format.js';
import { punchColumns } from './punchColumns.js';
import { pageSize } from './pageSize.js';
import { useAdjacentDays } from '../../useAdjacentDays.js';

type Card = DailyTimecard;
type Punch = Card['punches'][number];
const none: Card[] = [];
const noPunches: Punch[] = [];
const punchDetail: TableColumn<Punch>[] = [
  { id: 'in', header: 'In', cell: (punch) => punch.in ?? '—' },
  { id: 'out', header: 'Out', cell: (punch) => punch.out ?? '—' },
  { id: 'hours', header: 'Hours', cell: (punch) => punch.hours?.toFixed(2) ?? '—' },
];
export function TimecardsPage({
  date,
  onDateChange,
  refreshKey,
  timezone,
  preferences,
}: {
  date: string;
  onDateChange: (date: string) => void;
  refreshKey?: string | null;
  timezone: string;
  preferences: PaycomPreferences;
}) {
  const state = useTableState('timecard', { id: 'name', desc: false });
  const sort = state.sort!;
  const calendarToday = localDate(timezone);
  const [selected, setSelected] = useState<Card>();
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState('');
  const url = dailyPageUrl(date, sort.id, sort.desc, state.page, pageSize);
  const { data: current, stale, error } = useDailyTimecards(url, refreshKey);
  useAdjacentDays(url, date, calendarToday, current);
  // The previous day's rows hold the layout, dimmed and inert, until the new day arrives.
  const data = current ?? stale;
  useEffect(() => {
    if (current && state.page && state.page * pageSize >= current.total)
      state.setPage(Math.max(0, Math.ceil(current.total / pageSize) - 1));
  }, [current, state.page, state.setPage]);
  const columns = useMemo<TableColumn<Card>[]>(
    () => [
      {
        id: 'name',
        header: 'Employee',
        sortable: true,
        sticky: true,
        value: (card) => card.name,
        cell: (card) => (
          <button
            className="employee-timecard"
            aria-label={`View punches for ${card.name}`}
            onClick={() => setSelected(card)}
          >
            {card.name}
          </button>
        ),
      },
      ...punchColumns<Card>(true),
    ],
    [],
  );
  const table = useDataTable({
    columns,
    rows: data?.rows ?? none,
    total: data?.total,
    rowId: (card) => card.employeeCode,
    state,
    sorting: 'server',
    pageSize,
  });
  return (
    <div className="paycom-data-view">
      <div className="paycom-data-table paycom-timecard-table">
        <div className="paycom-table-heading paycom-timecard-heading">
          <div className="paycom-timecard-title">
            <h2>Employee timecards</h2>
            {data?.available && (
              <span className="paycom-employee-count">{data.total} employees</span>
            )}
          </div>
          <PaycomDateControls
            date={date}
            today={calendarToday}
            onChange={(value) => {
              onDateChange(value);
              state.setPage(0);
              setSelected(undefined);
            }}
          />
          <button
            className="icon-button"
            aria-label="Export timecards"
            disabled={!current?.total || exporting}
            aria-busy={exporting}
            onClick={async () => {
              setExporting(true);
              setExportError('');
              try {
                const full = await exportDailyTimecards(date, sort.id, sort.desc);
                downloadTable({ ...table, allRows: full.rows }, `timecards-${date}.csv`);
              } catch (error) {
                setExportError(error instanceof Error ? error.message : String(error));
              } finally {
                setExporting(false);
              }
            }}
          >
            <Download size={16} />
          </button>
        </div>
        {exportError && <ErrorBox message={exportError} />}
        <DataState data={data} error={error} failed={Boolean(error)}>
          {(data) =>
            !data.available ? (
              <Empty title="No collection covers this date">
                Choose another date or collect the current pay period.
              </Empty>
            ) : (
              <div className="paycom-day-results" aria-busy={!current} inert={!current}>
                <div className="table-wrap">
                  <DataTable
                    table={table}
                    stickyHeader
                    className="paycom-day-table"
                    label="Daily employee timecards"
                  />
                </div>
                <TablePagination table={table} />
                {!data.rows.length && (
                  <Empty
                    title={
                      preferences.driver_departments?.length === 0
                        ? 'No driver departments selected'
                        : 'No employees match your Timecard settings'
                    }
                  >
                    Your DSP owner can choose which departments appear in Timecard Settings.
                  </Empty>
                )}
              </div>
            )
          }
        </DataState>
        <footer className="paycom-timecard-footer" aria-label="Timecard timezones">
          <span>
            <Globe size={16} aria-hidden="true" />
            {timezone.replaceAll('_', ' ')}
          </span>
          <div className="paycom-timecard-business-time">
            <details className="paycom-timecard-info">
              <summary aria-label="About timecard data">
                <Info size={16} aria-hidden="true" />
              </summary>
              <p>
                Last completed collection {time(data?.collectedAt, timezone)}. Times update during
                collection, and hours may change after corrections.
              </p>
            </details>
          </div>
        </footer>
      </div>
      {selected && (
        <PunchDetail
          initial={selected}
          refreshKey={refreshKey}
          close={() => setSelected(undefined)}
        />
      )}
    </div>
  );
}

/** Its own read keeps an open dialog current even if the employee moves off the table's page. */
function PunchDetail({
  initial,
  refreshKey,
  close,
}: {
  initial: Card;
  refreshKey?: string | null;
  close: () => void;
}) {
  const request = useDailyPunches(initial.date, initial.employeeCode, refreshKey);
  const selected = request.data ?? request.stale ?? initial;
  const table = useDataTable({
    columns: punchDetail,
    rows: selected.punches ?? noPunches,
    rowId: (_, index) => String(index),
  });
  return (
    <Modal title={`${selected.name} · ${selected.date}`} onClose={close}>
      {request.error && <ErrorBox message={request.error} />}
      <div className="table-wrap">
        <DataTable table={table} />
      </div>
    </Modal>
  );
}
