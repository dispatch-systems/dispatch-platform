import { ChevronRight } from 'lucide-react';
import type { Driver, DriverSource } from '../../../shared/contracts/index.js';
import type { TableColumn } from '../../../core/shell/frontend/ui/index.js';
import { dataLabels, dayLabel, firstId, shortId, statusLabels } from './driver-match.js';
import { AppearsIn } from './AppearsIn.js';
import { DriverButton } from './DriverButton.js';
import { DriverTag } from './DriverTag.js';

/** Each source's ID beside the name as that source writes it. */
function sourceCell(driver: Driver, source: DriverSource) {
  const id = firstId(driver, source);
  if (!id) return <span className="driver-none">—</span>;
  return (
    <>
      <span className="driver-source-id" title={id.id}>
        {shortId(id)}
      </span>
      <small>{id.name}</small>
    </>
  );
}

/** The everyone table: who, their IDs, where they appear, when last seen and how matched. */
export function driverColumns(
  onOpen: (code: string) => void,
  today: string,
): TableColumn<Driver>[] {
  return [
    {
      id: 'name',
      header: 'Driver',
      sortable: true,
      value: (driver) => driver.name,
      exports: [
        ['Code', (driver) => driver.code],
        ['Name', (driver) => driver.name],
      ],
      cell: (driver) => (
        <DriverButton driver={driver} onOpen={onOpen}>
          <span className="driver-code">{driver.code}</span>
        </DriverButton>
      ),
    },
    {
      id: 'paycom',
      header: 'Paycom',
      sortable: true,
      value: (driver) => firstId(driver, 'paycom')?.id ?? '',
      cell: (driver) => sourceCell(driver, 'paycom'),
    },
    {
      id: 'amazon',
      header: 'Amazon',
      sortable: true,
      value: (driver) => firstId(driver, 'amazon')?.id ?? '',
      cell: (driver) => sourceCell(driver, 'amazon'),
    },
    {
      id: 'appears',
      header: 'Appears in',
      value: (driver) => driver.appears.map((data) => dataLabels[data]).join(', '),
      cell: (driver) => <AppearsIn data={driver.appears} />,
    },
    {
      id: 'seen',
      header: 'Last seen',
      sortable: true,
      value: (driver) => driver.lastSeen ?? '',
      cell: (driver) => <span className="driver-seen">{dayLabel(driver.lastSeen, today)}</span>,
    },
    {
      id: 'status',
      header: 'Match',
      sortable: true,
      value: (driver) => statusLabels[driver.status],
      cell: (driver) => <DriverTag status={driver.status} />,
    },
    {
      id: 'open',
      header: <span className="sr-only">Details</span>,
      className: 'cell-end',
      cell: (driver) => (
        <button
          className="driver-open"
          aria-label={`Open ${driver.name}`}
          onClick={() => onOpen(driver.code)}
        >
          <ChevronRight size={16} aria-hidden="true" />
        </button>
      ),
    },
  ];
}
