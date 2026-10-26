import { useState } from 'react';
import { UserPlus } from 'lucide-react';
import type { Conversation, RouteDriver } from '../../api/index.js';
import { pickDriver } from '../../api/client.js';
import { Popover, SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { clock } from '../format.js';
import { Face } from './Face.js';
import { RouteCode } from './RouteFacts.js';
import './pick-driver.css';

// Pick driver, for a work phone nobody held: one of today's drivers, found by name or route, each
// with their route and wave, under its head's right end. Picked, the phone's texts of today are
// theirs, and their conversation opens. It closes on Escape, focus going back to the button, a
// press anywhere else, or the button again.

export function PickDriver({
  conversation,
  drivers,
  timeZone,
  picked,
}: {
  conversation: Conversation;
  /** Today's drivers, by name. */
  drivers: readonly RouteDriver[];
  timeZone: string;
  /** Opens the driver's conversation, as picking left it. */
  picked: (conversation: Conversation) => void;
}) {
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState('');
  const pick = useAction(async (driver: RouteDriver) => {
    picked(await pickDriver(conversation.id, driver.driverId));
    setOpen(false);
  });
  const wanted = typed.trim().toLocaleLowerCase();
  const found = drivers.filter((driver) =>
    [driver.person.name, driver.today.route ?? ''].some((said) =>
      said.toLocaleLowerCase().includes(wanted),
    ),
  );
  return (
    <Popover
      className="pick-driver"
      panelClassName="pick-driver-panel"
      label="Pick driver"
      trigger={
        <>
          <UserPlus size={15} />
          <span>Pick driver</span>
        </>
      }
      open={open}
      onRequestOpen={() => setOpen(true)}
      onRequestClose={() => setOpen(false)}
    >
      <SearchInput label="Search drivers" placeholder="Search" value={typed} onChange={setTyped} />
      <h3>Today's Routes</h3>
      {found.length === 0 ? (
        <p className="list-empty">No matches</p>
      ) : (
        <ul>
          {found.map((driver) => (
            <li key={driver.driverId}>
              <button disabled={pick.busy} onClick={() => void pick.run(driver)}>
                <Face {...driver.person} size="contact" />
                <span>
                  <strong>{driver.person.name}</strong>
                  <small>
                    {driver.today.route && <RouteCode code={driver.today.route} />}
                    {driver.today.waveAt !== null && clock(driver.today.waveAt, timeZone)}
                  </small>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Popover>
  );
}
