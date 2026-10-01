import { useState } from 'react';
import type { Driver } from '../../../../shared/contracts/index.js';
import { mergeDrivers } from '../../app/endpoints.js';
import { useAction } from '../../app/useAction.js';
import { driverMatches, statusLabels } from '../../lib/driver-match.js';
import { ErrorBox, Modal, SearchInput } from '../../ui/index.js';
import { DriverAvatar } from './DriverAvatar.js';

/** Chooses who else is this person: their IDs move here, and their code leads here. */
export function MergeDialog({
  driver,
  drivers,
  onClose,
}: {
  driver: Driver;
  drivers: Driver[];
  onClose: () => void;
}) {
  const [query, setQuery] = useState('');
  const [chosen, setChosen] = useState('');
  const merge = useAction((other: Driver) => mergeDrivers(other.code, driver.code), {
    inline: true,
    success: (other) => `${other.name} and ${driver.name} are now one person`,
  });
  const options = drivers
    .filter((other) => other.code !== driver.code && driverMatches(other, query))
    .slice(0, 8);
  const other = drivers.find((d) => d.code === chosen);
  return (
    <Modal
      title={`Merge with ${driver.name}`}
      description={`Choose who is the same person. Their IDs move to ${driver.name}, who keeps code ${driver.code}; their code leads here from then on.`}
      onClose={onClose}
    >
      <SearchInput
        label="Search drivers"
        placeholder="Search name, code or ID"
        value={query}
        onChange={setQuery}
      />
      <div className="driver-merge-list" role="radiogroup" aria-label="Drivers">
        {options.map((option) => (
          <label key={option.code}>
            <input
              type="radio"
              name="driver-merge"
              checked={chosen === option.code}
              onChange={() => setChosen(option.code)}
            />
            <DriverAvatar name={option.name} code={option.code} />
            <span className="driver-person">
              <strong>{option.name}</strong>
              <small>
                {option.code} · {statusLabels[option.status]}
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
