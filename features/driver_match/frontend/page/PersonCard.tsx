import { Check, X } from 'lucide-react';
import type { PeopleSource, Person } from '../../api/index.js';
import { knows, roleLabel, sourceLabel, working } from '../driver-match.js';
import { Face } from '../parts/Face.js';

/** One person: their face, what they do, which sources have them, and a question about them. */
export function PersonCard({
  person,
  sources,
  queued,
  onOpen,
  onCheck,
}: {
  person: Person;
  sources: readonly PeopleSource[];
  queued: boolean;
  onOpen: (code: string) => void;
  onCheck: (code: string) => void;
}) {
  const listed = working(person) ? sources : sources.filter((source) => knows(person, source));
  return (
    <li className={`driver-card${queued ? ' queued' : ''}`}>
      <button type="button" className="driver-card-person" onClick={() => onOpen(person.code)}>
        <Face person={person} size="portrait" />
        <strong>{person.name}</strong>
        <span>{roleLabel(person)}</span>
      </button>
      {listed.length > 0 && (
        <ul className="driver-card-sources">
          {listed.map((source) =>
            knows(person, source) ? (
              <li key={source}>
                <Check size={15} aria-hidden="true" />
                {sourceLabel(source)}
              </li>
            ) : (
              <li key={source} className="missing">
                <X size={15} aria-hidden="true" />
                Not in {sourceLabel(source)}
              </li>
            ),
          )}
        </ul>
      )}
      {queued && (
        <button className="driver-card-check" onClick={() => onCheck(person.code)}>
          Check
        </button>
      )}
    </li>
  );
}
