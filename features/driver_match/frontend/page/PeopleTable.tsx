import { X } from 'lucide-react';
import { Fragment } from 'react';
import type { PeopleSource, Person } from '../../api/index.js';
import {
  DataTable,
  TablePagination,
  useDataTable,
  type TableColumn,
  type TableStateStore,
} from '../../../../core/shell/frontend/ui/index.js';
import { day, knows, lastSeen, roleLabel, shortId, sourceLabel, working } from '../driver-match.js';
import { Face } from '../parts/Face.js';

/** Rows a page. */
const PAGE = 100;

/**
 * Everyone as a list: their face and name, what they do, their IDs in each system or the
 * systems that lack a driver, when they were last seen, and Check while a question waits.
 */
export function PeopleTable({
  people,
  sources,
  queued,
  state,
  today,
  onOpen,
  onCheck,
}: {
  people: Person[];
  sources: readonly PeopleSource[];
  /** Who a question waits on, by code. */
  queued: ReadonlySet<string>;
  /** Its sort and page, which a new filter or search starts over. */
  state: TableStateStore;
  today: string;
  onOpen: (code: string) => void;
  onCheck: (code: string) => void;
}) {
  const columns: TableColumn<Person>[] = [
    {
      id: 'name',
      header: 'Name',
      scope: 'col',
      rowHeader: true,
      sortable: true,
      value: (person) => person.name,
      cell: (person) => (
        <button type="button" className="driver-list-person" onClick={() => onOpen(person.code)}>
          <Face person={person} />
          <span>
            <strong>{person.name}</strong>
            <Under person={person} sources={sources} />
          </span>
        </button>
      ),
    },
    {
      id: 'role',
      header: 'Role',
      scope: 'col',
      sortable: true,
      className: 'driver-list-role',
      value: roleLabel,
      cell: roleLabel,
    },
    ...sources.map((source): TableColumn<Person> => ({
      id: source,
      header: sourceLabel(source),
      scope: 'col',
      className: 'driver-list-ids',
      cell: (person) => <Ids person={person} source={source} />,
    })),
    {
      id: 'seen',
      header: 'Last seen',
      scope: 'col',
      sortable: true,
      className: 'driver-list-seen',
      value: (person) => lastSeen(person) ?? '',
      cell: (person) => {
        const seen = lastSeen(person);
        return seen ? day(seen, today) : '';
      },
    },
    {
      id: 'check',
      header: <span className="sr-only">Check</span>,
      scope: 'col',
      className: 'driver-list-check',
      cell: (person) =>
        queued.has(person.code) && <button onClick={() => onCheck(person.code)}>Check</button>,
    },
  ];
  const table = useDataTable({
    columns,
    rows: people,
    rowId: (person) => person.code,
    state,
    pageSize: PAGE,
  });
  return (
    <>
      <DataTable
        table={table}
        label="People"
        className="driver-list"
        stickyHeader
        rowClassName={(person) => (queued.has(person.code) ? 'queued' : undefined)}
      />
      <TablePagination table={table} />
    </>
  );
}

/** Their IDs in one system; for a driver or helper it lacks, that it does. */
function Ids({ person, source }: { person: Person; source: PeopleSource }) {
  const ids = person.ids.filter((id) => id.source === source);
  const [first, ...more] = ids;
  if (first)
    return (
      <span className="driver-source-id" title={ids.map((id) => id.id).join(', ')}>
        {shortId(first)}
        {more.length > 0 && ` +${more.length}`}
      </span>
    );
  if (!working(person)) return null;
  return (
    <span className="driver-list-missing">
      <X size={14} aria-hidden="true" />
      Not in {sourceLabel(source)}
    </span>
  );
}

/** On a phone, what the hidden columns say, under the name: what they do and what lacks them. */
function Under({ person, sources }: { person: Person; sources: readonly PeopleSource[] }) {
  const lacking = working(person) ? sources.filter((source) => !knows(person, source)) : [];
  return (
    <small className="driver-list-under">
      {roleLabel(person)}
      {lacking.map((source) => (
        <Fragment key={source}>
          {' · '}
          <span className="driver-list-missing">Not in {sourceLabel(source)}</span>
        </Fragment>
      ))}
    </small>
  );
}
