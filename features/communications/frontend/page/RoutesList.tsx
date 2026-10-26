import { Route } from 'lucide-react';
import type { Conversation, Writing } from '../../api/index.js';
import { clock } from '../format.js';
import type { RouteGroup, RouteHeading, RouteRow } from '../todays-routes.js';
import { rowKey } from '../todays-routes.js';
import { ConversationRow } from './ConversationList.js';
import { Face } from './Face.js';
import { ListRow } from './ListRow.js';
import { PaceMark, RouteCode, Stops, WorkPhone } from './RouteFacts.js';
import './routes-list.css';

// Today's Routes in the list: its groups, Waiting, each wave, no wave and no driver, each row the
// driver with their route, work phone, stops and pace under what was last written, or a work phone
// nobody held. Empty, as once live route data holds no day after the stop: No routes.

export function RoutesList({
  groups,
  open,
  openDriver,
  typing,
  timeZone,
  now,
  choose,
  done,
}: {
  groups: readonly RouteGroup[];
  /** What's open: a conversation, by its ID, and the driver it's with, if it's a driver's. */
  open?: string;
  openDriver?: string | null;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  /** Opens a row's conversation, or starts a driver's. */
  choose: (row: RouteRow) => void;
  done: (conversation: Conversation) => void;
}) {
  if (groups.length === 0) return <NoRoutes />;
  const listing = { open, typing, timeZone, now, done };
  return (
    <>
      {groups.map((group) => (
        <section key={group.key} aria-label={headingText(group.heading, timeZone)}>
          <h2 className="list-head">
            {headingText(group.heading, timeZone)}
            <span>{group.rows.length}</span>
          </h2>
          <ul className="convos">
            {group.rows.map((row) => {
              const below = row.kind === 'driver' && <RouteLine row={row} />;
              return row.conversation ? (
                <ConversationRow
                  key={rowKey(row)}
                  conversation={row.conversation}
                  below={below}
                  select={() => choose(row)}
                  {...listing}
                />
              ) : (
                row.kind === 'driver' && (
                  <ListRow
                    key={rowKey(row)}
                    selected={row.driver.driverId === openDriver}
                    waits={false}
                    face={<Face {...row.driver.person} size="list" />}
                    name={row.driver.person.name}
                    below={below}
                    open={() => choose(row)}
                  />
                )
              );
            })}
          </ul>
        </section>
      ))}
    </>
  );
}

/** A driver's route under their row: its code, their work phone, the stops done and its pace. */
function RouteLine({ row }: { row: Extract<RouteRow, { kind: 'driver' }> }) {
  const { today } = row.driver;
  return (
    <>
      {today.route && <RouteCode code={today.route} />}
      {today.phone && <WorkPhone number={today.phone} />}
      <Stops today={today} />
      <PaceMark pace={today.pace} />
    </>
  );
}

/** What a group of rows is called: Waiting, its wave by when it starts, No wave, or No driver. */
export function headingText(heading: RouteHeading, timeZone: string) {
  switch (heading.kind) {
    case 'waiting':
      return 'Waiting';
    case 'wave':
      return clock(heading.at, timeZone);
    case 'no_wave':
      return 'No wave';
    case 'no_driver':
      return 'No driver';
  }
}

/** Today's Routes with nobody on it. */
export function NoRoutes() {
  return (
    <div className="no-routes">
      <Route aria-hidden="true" />
      No routes
    </div>
  );
}
