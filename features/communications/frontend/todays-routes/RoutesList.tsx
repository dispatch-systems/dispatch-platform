import type { Conversation, Writing } from '../../api/index.js';
import { ConversationRow } from '../inbox/ConversationRow.js';
import { Face } from '../shared/Face.js';
import { ListHead, Rows } from '../shared/List.js';
import { ListRow } from '../shared/ListRow.js';
import { groupHeading, RoutesState, type RoutesRead } from './headings.js';
import { rowKey, type RouteGroup, type RouteRow } from './todays-routes.js';
import { RouteLine } from './RouteFacts.js';

// Today's Routes in the list: its drivers A to Z, then No driver, each row the driver with their
// route, work phone, stops and pace under what was last written, or a work phone nobody held;
// while it's first read, loading; its read failing, why; empty, as once live route data holds no
// day after the stop, No routes.

export function RoutesList({
  groups,
  read,
  open,
  openDriver,
  typing,
  timeZone,
  now,
  choose,
  warm,
  done,
}: {
  groups: readonly RouteGroup[];
  read: RoutesRead;
  /** What's open: a conversation, by its ID, and the driver it's with, if it's a driver's. */
  open?: string;
  openDriver?: string | null;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  /** Opens a row's conversation, or starts a driver's. */
  choose: (row: RouteRow) => void;
  /** Reads a conversation before it's opened, as its row is pointed at or reached. */
  warm: (id: string) => void;
  done: (conversation: Conversation) => void;
}) {
  const listing = { open, typing, timeZone, now, warm, done };
  return (
    <RoutesState read={read} empty={groups.length === 0}>
      {groups.map((group) => (
        <section key={group.key} aria-label={groupHeading(group)}>
          {groupHeading(group) && (
            <ListHead count={group.rows.length}>{groupHeading(group)}</ListHead>
          )}
          <Rows>
            {group.rows.map((row) => {
              const below = row.kind === 'driver' && <RouteLine today={row.driver.today} />;
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
          </Rows>
        </section>
      ))}
    </RoutesState>
  );
}
