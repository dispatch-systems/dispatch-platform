import { Check, X } from 'lucide-react';
import type { Driver, DriverSource } from '../../api/index.js';
import { knows, roleLabels, sourceLabel, working } from '../driver-match.js';
import { DriverAvatar } from '../parts/DriverAvatar.js';

/** One person: their face, what they do, which sources have them, and a question about them. */
export function PersonCard({
  driver,
  sources,
  queued,
  onOpen,
  onCheck,
}: {
  driver: Driver;
  sources: readonly DriverSource[];
  queued: boolean;
  onOpen: (code: string) => void;
  onCheck: (code: string) => void;
}) {
  const listed = working(driver) ? sources : sources.filter((source) => knows(driver, source));
  return (
    <li className={`driver-card${queued ? ' queued' : ''}`}>
      <button type="button" className="driver-card-person" onClick={() => onOpen(driver.code)}>
        <DriverAvatar driver={driver} size="portrait" />
        <strong>{driver.name}</strong>
        <span>{roleLabels[driver.role]}</span>
      </button>
      {listed.length > 0 && (
        <ul className="driver-card-sources">
          {listed.map((source) =>
            knows(driver, source) ? (
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
        <button className="driver-card-check" onClick={() => onCheck(driver.code)}>
          Check
        </button>
      )}
    </li>
  );
}
