import { ChevronDown } from 'lucide-react';
import { useState } from 'react';
import type { Driver, DriverId } from '../../api/index.js';
import { confirmDriver, splitDriver } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { goesBy, shortId, sourceLabel } from '../driver-match.js';
import { DriverButton } from '../parts/DriverButton.js';

/** The IDs a nickname joined to someone, which nobody has confirmed yet. */
const variants = (driver: Driver) => driver.ids.filter((id) => id.linkedBy === 'variant');

/** People joined by a nickname alone, to confirm or part, folded until opened. */
export function NicknameList({
  drivers,
  onOpen,
}: {
  drivers: Driver[];
  onOpen: (code: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const joined = drivers.filter((driver) => variants(driver).length > 0);
  const confirm = useAction(
    (driver: Driver, id: DriverId) => confirmDriver(driver.code, id.source, id.id),
    { inline: true, success: (driver) => `${driver.name} is confirmed` },
  );
  const part = useAction(
    (driver: Driver, id: DriverId) => splitDriver(driver.code, id.source, id.id),
    {
      inline: true,
      success: (_, id) => `${sourceLabel(id.source)} ID ${shortId(id)} is now its own person`,
    },
  );
  if (!joined.length) return null;
  const busy = confirm.busy || part.busy;
  return (
    <section className="driver-nicknames" aria-labelledby="driver-nicknames-title">
      <button
        type="button"
        className="driver-nicknames-head"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <ChevronDown size={16} aria-hidden="true" />
        <h2 id="driver-nicknames-title">Linked by nickname</h2>
        <span className="driver-count">{joined.length}</span>
      </button>
      <ErrorBox message={confirm.error || part.error} />
      {open &&
        joined.flatMap((driver) =>
          variants(driver).map((id) => {
            const alias = goesBy(driver);
            return (
              <div className="driver-nickname" key={`${driver.code}:${id.source}:${id.id}`}>
                <DriverButton driver={driver} onOpen={onOpen}>
                  <small>{alias ? `${alias.name} on ${alias.source}` : driver.code}</small>
                </DriverButton>
                <button disabled={busy} onClick={() => void part.run(driver, id)}>
                  Not them
                </button>
                <button disabled={busy} onClick={() => void confirm.run(driver, id)}>
                  Confirm
                </button>
              </div>
            );
          }),
        )}
    </section>
  );
}
