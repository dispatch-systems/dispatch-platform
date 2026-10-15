import { CircleCheck, CircleHelp } from 'lucide-react';
import type { Driver, DriverSource } from '../../api/index.js';
import type { TableColumn } from '../../../../core/shell/frontend/ui/index.js';
import { dayLabel, goesBy, linkLabels, roleLabels, sourceLabel } from '../driver-match.js';
import { DriverButton } from '../parts/DriverButton.js';
import { LinkTag, RoleTag } from '../parts/Tags.js';

/** Whether a source knows someone: yes, maybe while a pair waits, or not. */
function sourceCell(driver: Driver, source: DriverSource) {
  const id = driver.ids.find((each) => each.source === source);
  if (id)
    return (
      <span className="driver-known" title={`${id.id}${id.name ? ` · ${id.name}` : ''}`}>
        <CircleCheck size={15} aria-label={`${sourceLabel(source)} knows them`} />
      </span>
    );
  if (driver.review)
    return (
      <span className="driver-maybe">
        <CircleHelp size={15} aria-label={`Maybe in ${sourceLabel(source)}`} />
      </span>
    );
  return <span className="driver-none">—</span>;
}

/** The people table: who, which sources know them, what they do, how sure, and when seen. */
export function peopleColumns(
  sources: readonly DriverSource[],
  onOpen: (code: string) => void,
  today: string,
): TableColumn<Driver>[] {
  return [
    {
      id: 'name',
      header: 'Person',
      sortable: true,
      value: (driver) => driver.name,
      exports: [
        ['Code', (driver) => driver.code],
        ['Name', (driver) => driver.name],
      ],
      cell: (driver) => {
        const alias = goesBy(driver);
        return (
          <DriverButton driver={driver} onOpen={onOpen}>
            <small>
              <span className="driver-code">{driver.code}</span>
              {alias && ` ${alias.name} on ${alias.source}`}
            </small>
          </DriverButton>
        );
      },
    },
    ...sources.map((source): TableColumn<Driver> => ({
      id: source,
      header: sourceLabel(source),
      className: 'driver-source-cell',
      value: (driver) => driver.ids.find((id) => id.source === source)?.id ?? '',
      cell: (driver) => sourceCell(driver, source),
    })),
    {
      id: 'role',
      header: 'Role',
      sortable: true,
      value: (driver) => roleLabels[driver.role],
      cell: (driver) => <RoleTag role={driver.role} />,
    },
    {
      id: 'link',
      header: 'Linked by',
      sortable: true,
      value: (driver) => (driver.review ? 'Needs review' : linkLabels[driver.link]),
      cell: (driver) =>
        driver.review || driver.link !== 'new' ? (
          <LinkTag link={driver.link} review={driver.review} />
        ) : (
          <span className="driver-none">—</span>
        ),
    },
    {
      id: 'seen',
      header: 'Last seen',
      sortable: true,
      value: (driver) => driver.lastSeen ?? '',
      cell: (driver) => <span className="driver-seen">{dayLabel(driver.lastSeen, today)}</span>,
    },
  ];
}
