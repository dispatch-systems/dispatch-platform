import { useDeferredValue, useMemo, useState } from 'react';
import type { Driver, DriverSource } from '../../api/index.js';
import { useUpdateState } from '../../../../core/shell/frontend/runtime/browser-update.js';
import { Empty, SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import { driverSearchTerms, missing, peopleFilters, type PeopleFilter } from '../driver-match.js';
import { PersonCard } from './PersonCard.js';

/** How many cards show before more are asked for: whole rows, two to eight across. */
const PAGE = 48;

/** Everyone, a card each, by what they do or a source lacking them, and found by search. */
export function PeopleGrid({
  drivers,
  sources,
  queued,
  onOpen,
  onCheck,
}: {
  drivers: Driver[];
  sources: readonly DriverSource[];
  /** Who a question waits on, by code. */
  queued: ReadonlySet<string>;
  onOpen: (code: string) => void;
  onCheck: (code: string) => void;
}) {
  const [query, setQuery] = useUpdateState('driver-match-query', '');
  const [filter, setFilter] = useUpdateState<PeopleFilter>('driver-match-people', 'all');
  const [shown, setShown] = useState(PAGE);
  // What each filter would show, counted once per answer, with what a search reads.
  const prepared = useMemo(() => {
    // Someone who has left shows only under Former.
    const shows = (key: PeopleFilter, driver: Driver) =>
      key === 'all'
        ? driver.role !== 'former'
        : key === 'missing'
          ? missing(driver, sources)
          : driver.role === key;
    const searchable = drivers.map((driver) => ({ driver, terms: driverSearchTerms(driver) }));
    const counts = Object.fromEntries(
      peopleFilters.map(([key]) => [key, drivers.filter((driver) => shows(key, driver)).length]),
    ) as Record<PeopleFilter, number>;
    return { shows, searchable, counts };
  }, [drivers, sources]);
  const search = query.trim().toLocaleLowerCase('en-US');
  const deferredSearch = useDeferredValue(search);
  const searching = search !== deferredSearch;
  const people = useMemo(
    () =>
      prepared.searchable
        .filter(
          ({ driver, terms }) =>
            prepared.shows(filter, driver) && terms.some((text) => text.includes(deferredSearch)),
        )
        .map(({ driver }) => driver),
    [prepared, filter, deferredSearch],
  );
  return (
    <section aria-label="People">
      <div className="driver-tools">
        <div className="driver-filters" role="group" aria-label="Show">
          {peopleFilters.map(([key, text]) => (
            <button
              key={key}
              aria-pressed={filter === key}
              onClick={() => {
                setFilter(key);
                setShown(PAGE);
              }}
            >
              {text}
              <span>{prepared.counts[key]}</span>
            </button>
          ))}
        </div>
        <SearchInput
          label="Search people"
          placeholder="Search"
          value={query}
          onChange={(value) => {
            setQuery(value);
            setShown(PAGE);
          }}
        />
      </div>
      {drivers.length === 0 ? (
        <Empty title="No one to match yet">
          Codes appear after the first collection that names someone.
        </Empty>
      ) : people.length === 0 ? (
        <Empty title="No matching people">Try another name or filter.</Empty>
      ) : (
        <ul className="driver-cards" aria-busy={searching} inert={searching}>
          {people.slice(0, shown).map((driver) => (
            <PersonCard
              key={driver.code}
              driver={driver}
              sources={sources}
              queued={queued.has(driver.code)}
              onOpen={onOpen}
              onCheck={onCheck}
            />
          ))}
        </ul>
      )}
      {people.length > shown && (
        <button className="driver-more" onClick={() => setShown(shown + PAGE)}>
          Show more
        </button>
      )}
    </section>
  );
}
