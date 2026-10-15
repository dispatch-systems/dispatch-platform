import { ChevronDown } from 'lucide-react';
import { useState } from 'react';
import type { Driver, DriverId } from '../../api/index.js';
import { confirmDriver, splitDriver } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { shortId, sourceLabel } from '../driver-match.js';
import { DriverButton } from '../parts/DriverButton.js';

/** The IDs a name written another way joined to someone, which nobody has confirmed yet. */
const variants = (driver: Driver) => driver.ids.filter((id) => id.linkedBy === 'variant');

/**
 * People joined by a name written another way alone, as by a second surname or a middle name, to
 * confirm or part, folded until opened.
 */
export function VariantList({
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
    <section className="driver-variants" aria-labelledby="driver-variants-title">
      <h2 className="driver-variants-head">
        <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>
          <ChevronDown size={16} aria-hidden="true" />
          <span id="driver-variants-title">Name variants</span>
          <span className="driver-count">{joined.length}</span>
        </button>
      </h2>
      <ErrorBox message={confirm.error || part.error} />
      {open &&
        joined.flatMap((driver) =>
          variants(driver).map((id) => (
            <div className="driver-variant" key={`${driver.code}:${id.source}:${id.id}`}>
              <DriverButton driver={driver} onOpen={onOpen}>
                <small>
                  {id.name || shortId(id)} on {sourceLabel(id.source)}
                </small>
              </DriverButton>
              <button disabled={busy} onClick={() => void part.run(driver, id)}>
                Not them
              </button>
              <button disabled={busy} onClick={() => void confirm.run(driver, id)}>
                Confirm
              </button>
            </div>
          )),
        )}
    </section>
  );
}
