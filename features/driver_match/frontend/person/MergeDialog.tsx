import { useState } from 'react';
import type { Person } from '../../api/index.js';
import { mergePeople } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { found, roles, searchTerms } from '../driver-match.js';
import { ErrorBox, Modal, SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import { Face } from '../parts/Face.js';

/** Chooses who else is this person: their IDs move here, and their code leads here. */
export function MergeDialog({
  person,
  people,
  onClose,
}: {
  person: Person;
  people: Person[];
  onClose: () => void;
}) {
  const [query, setQuery] = useState('');
  const [chosen, setChosen] = useState('');
  const merge = useAction((other: Person) => mergePeople(other.code, person.code), {
    inline: true,
    success: (other) => `${other.name} and ${person.name} are now one person`,
  });
  const options = people
    .filter((other) => other.code !== person.code && found(searchTerms(other), query))
    .slice(0, 8);
  const other = people.find((p) => p.code === chosen);
  return (
    <Modal title={`Merge with ${person.name}`} onClose={onClose}>
      <SearchInput
        label="Search people"
        placeholder="Search name, code or ID"
        value={query}
        onChange={setQuery}
      />
      <div className="driver-merge-list" role="radiogroup" aria-label="People">
        {options.map((option) => (
          <label key={option.code}>
            <input
              type="radio"
              name="driver-merge"
              checked={chosen === option.code}
              onChange={() => setChosen(option.code)}
            />
            <Face person={option} />
            <span className="driver-person">
              <strong>{option.name}</strong>
              <small>
                {option.code} · {roles[option.role].label}
              </small>
            </span>
          </label>
        ))}
        {!options.length && <p className="muted">No one else matches that search.</p>}
      </div>
      <ErrorBox message={merge.error} />
      <div className="form-actions">
        <button onClick={onClose}>Cancel</button>
        <button
          className="primary"
          disabled={!other || merge.busy}
          onClick={async () => {
            if (other && (await merge.run(other))) onClose();
          }}
        >
          Merge
        </button>
      </div>
    </Modal>
  );
}
