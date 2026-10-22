import { useDeferredValue, useMemo, useState } from 'react';
import type { PeopleSource, Person } from '../../api/index.js';
import { useUpdateState } from '../../../../core/shell/frontend/runtime/browser-update.js';
import { useTableState } from '../../../../core/shell/frontend/runtime/useTableState.js';
import { Empty, SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import { found, peopleFilters, searchTerms, shows, type PeopleFilter } from '../driver-match.js';
import { PeopleTable } from './PeopleTable.js';
import { PersonCard } from './PersonCard.js';
import type { View } from './ViewSwitch.js';

/** How many cards show before more are asked for: whole rows, two to eight across. */
const PAGE = 48;

/**
 * Everyone, by what they do or a source lacking them, and found by search: a row each, or a card
 * each.
 */
export function Everyone({
  people,
  sources,
  queued,
  view,
  today,
  onOpen,
  onCheck,
}: {
  people: Person[];
  sources: readonly PeopleSource[];
  /** Who a question waits on, by code. */
  queued: ReadonlySet<string>;
  view: View;
  today: string;
  onOpen: (code: string) => void;
  onCheck: (code: string) => void;
}) {
  const [query, setQuery] = useUpdateState('driver-match-query', '');
  const [filter, setFilter] = useUpdateState<PeopleFilter>('driver-match-people', 'all');
  const [shown, setShown] = useState(PAGE);
  const list = useTableState('driver-match-list');
  // A new filter or search starts from the first of them.
  const restart = () => {
    setShown(PAGE);
    list.setPage(0);
  };
  // What each filter would show, counted once per answer, with what a search reads.
  const prepared = useMemo(() => {
    const searchable = people.map((person) => ({ person, terms: searchTerms(person) }));
    const counts = Object.fromEntries(
      peopleFilters.map(([key]) => [key, people.filter((p) => shows(key, p, sources)).length]),
    ) as Record<PeopleFilter, number>;
    return { searchable, counts };
  }, [people, sources]);
  const deferredQuery = useDeferredValue(query);
  const searching = query !== deferredQuery;
  const listed = useMemo(
    () =>
      prepared.searchable
        .filter(
          ({ person, terms }) => shows(filter, person, sources) && found(terms, deferredQuery),
        )
        .map(({ person }) => person),
    [prepared, filter, sources, deferredQuery],
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
                restart();
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
            restart();
          }}
        />
      </div>
      {people.length === 0 ? (
        <Empty title="No one to match yet" />
      ) : listed.length === 0 ? (
        <Empty title="No matching people" />
      ) : view === 'list' ? (
        <div aria-busy={searching} inert={searching}>
          <PeopleTable
            people={listed}
            sources={sources}
            queued={queued}
            state={list}
            today={today}
            onOpen={onOpen}
            onCheck={onCheck}
          />
        </div>
      ) : (
        <ul className="driver-cards" aria-busy={searching} inert={searching}>
          {listed.slice(0, shown).map((person) => (
            <PersonCard
              key={person.code}
              person={person}
              sources={sources}
              queued={queued.has(person.code)}
              onOpen={onOpen}
              onCheck={onCheck}
            />
          ))}
        </ul>
      )}
      {view === 'cards' && listed.length > shown && (
        <button className="driver-more" onClick={() => setShown(shown + PAGE)}>
          Show more
        </button>
      )}
    </section>
  );
}
