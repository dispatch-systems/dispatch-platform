import { useEffect, useId, useState } from 'react';
import type { ConductPerson } from '../../api/index.js';
import { Person } from '../parts.js';

export function DriverPicker({
  people,
  value,
  change,
}: {
  people: ConductPerson[];
  value: string;
  change: (code: string) => void;
}) {
  const id = useId();
  const [query, setQuery] = useState<string>();
  const [active, setActive] = useState(0);
  const choices = people.filter((p) =>
    p.name.toLocaleLowerCase().includes((query ?? '').toLocaleLowerCase()),
  );
  useEffect(() => {
    if (query !== undefined)
      document.getElementById(`${id}-${active}`)?.scrollIntoView({ block: 'nearest' });
  }, [query, active, id]);
  const choose = (person: ConductPerson) => {
    change(person.code);
    setQuery(undefined);
    setActive(0);
  };
  return (
    <div className="conduct-driver-picker">
      <input
        role="combobox"
        aria-label="Driver"
        aria-expanded={query !== undefined}
        aria-controls={id}
        aria-autocomplete="list"
        aria-activedescendant={
          query !== undefined && choices[active] ? `${id}-${active}` : undefined
        }
        placeholder="Choose a driver"
        value={query ?? people.find((p) => p.code === value)?.name ?? ''}
        onFocus={() => {
          setQuery('');
          setActive(0);
        }}
        onBlur={() => setQuery(undefined)}
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
          if (!e.target.value) change('');
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.stopPropagation();
            setQuery(undefined);
          }
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            setQuery((q) => q ?? '');
            setActive((at) =>
              Math.max(0, Math.min(choices.length - 1, at + (e.key === 'ArrowDown' ? 1 : -1))),
            );
          }
          if (e.key === 'Enter' && query !== undefined && choices[active]) {
            e.preventDefault();
            choose(choices[active]);
          }
        }}
      />
      {query !== undefined && (
        <div id={id} role="listbox" aria-label="Drivers" className="conduct-driver-options">
          {choices.map((person, at) => (
            <button
              key={person.code}
              type="button"
              role="option"
              id={`${id}-${at}`}
              aria-selected={at === active}
              tabIndex={-1}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(person)}
            >
              <Person person={person} />
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
