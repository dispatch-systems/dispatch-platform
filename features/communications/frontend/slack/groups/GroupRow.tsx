import type { Group } from '../../../api/index.js';
import type { Listing } from '../../inbox/listing.js';
import { listTime, preview } from '../../shared/format.js';
import { ListRow } from '../../shared/ListRow.js';
import { Wait } from '../../shared/Wait.js';
import { GroupFaces } from './GroupFaces.js';

/**
 * A group in the list: two of its people, its name and how many it went to, when it was last
 * written in, and what was, with its writer's first name. While anything under it waits, how
 * long the longest has, and a check that marks all of it done.
 */
export function GroupRow({
  group,
  open,
  timeZone,
  now,
  openGroup,
  doneGroup,
}: Listing & { group: Group }) {
  const waits = group.state === 'waiting';
  const last = group.last;
  const writer = last?.sender?.name ?? last?.from?.name ?? '';
  return (
    <ListRow
      className="group"
      selected={open === group.id}
      waits={waits}
      face={<GroupFaces faces={group.faces} size="list" />}
      name={
        <>
          {group.name}
          <span className="people"> · {group.people}</span>
        </>
      }
      time={listTime(group.lastAt, timeZone, now)}
      wait={waits && group.waitingSince !== null && <Wait since={group.waitingSince} now={now} />}
      line={
        <span className="preview">
          {last ? `${writer.split(' ')[0] ?? writer}: ${preview(last)}` : ''}
        </span>
      }
      open={() => openGroup(group.id)}
      done={() => doneGroup(group)}
    />
  );
}
