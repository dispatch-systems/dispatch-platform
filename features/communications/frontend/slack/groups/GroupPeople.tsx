import { useId } from 'react';
import { Check, LoaderCircle, Reply, X, type LucideIcon } from 'lucide-react';
import type { Group, GroupPerson, GroupPersonState } from '../../../api/index.js';
import { useGroupPeople } from '../../../api/client.js';
import { ErrorBox, Loading } from '../../../../../core/shell/frontend/ui/index.js';
import { listTime } from '../../shared/format.js';
import { Face } from '../../shared/Face.js';
import { GroupFaces } from './GroupFaces.js';
import { Panel } from '../../shared/Panel.js';
import { roleText } from '../../shared/format.js';
import './group-people.css';

// Who a group went to, in the profile's place: the group, who first sent it and when; then each
// of its people, with what they do and where the group stands with them, each opening their own
// conversation. Those who answered come first.

/** Where the group stands with someone, as it reads. */
const states: Record<GroupPersonState, [LucideIcon, string]> = {
  replied: [Reply, 'Replied'],
  sending: [LoaderCircle, 'Sending'],
  sent: [Check, 'Sent'],
  not_sent: [X, 'Not sent'],
};

export function GroupPeople({
  id,
  group,
  refresh,
  timeZone,
  now,
  docked,
  leaving,
  still,
  close,
  gone,
  open,
}: {
  /** The panel's own ID, which its openers control. */
  id: string;
  group: Group;
  /** What changes as the group does, reading its people again. */
  refresh: string;
  timeZone: string;
  now: number;
  docked: boolean;
  /** Whether it's closed, sliding away. */
  leaving: boolean;
  /** Whether it's kept open as the conversation opened, there without sliding in. */
  still: boolean;
  close: (refocus: boolean) => void;
  /** Says it has slid away. */
  gone: () => void;
  /** Opens someone's own conversation. */
  open: (person: GroupPerson) => void;
}) {
  const { data, error } = useGroupPeople(group.id, refresh);
  const title = useId();
  return (
    <Panel
      id={id}
      labelledBy={title}
      className="people"
      docked={docked}
      leaving={leaving}
      still={still}
      close={close}
      gone={gone}
    >
      <div className="me">
        <GroupFaces faces={group.faces} size="profile" />
        <div>
          <h2 id={title}>{group.name}</h2>
          {data && (
            <small>
              Sent by {data.sentBy} · {listTime(data.sentAt, timeZone, now)}
            </small>
          )}
        </div>
      </div>
      <ErrorBox message={error} />
      {!data && !error && <Loading />}
      {data && (
        <ul className="recipients">
          {data.people.map((each) => {
            const [Icon, label] = states[each.state];
            return (
              <li key={`${each.teamId}:${each.person.userId}`}>
                <button className="recipient" onClick={() => open(each)}>
                  <Face {...each.person} size="message" />
                  <span className="recipient-body">
                    <strong>{each.person.name}</strong>
                    <small>{roleText(each.person)}</small>
                  </span>
                  <span className={`state ${each.state}`}>
                    <Icon size={13} aria-hidden="true" />
                    {label}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </Panel>
  );
}
