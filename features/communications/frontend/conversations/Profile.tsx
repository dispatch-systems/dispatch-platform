import { useId, type ReactNode } from 'react';
import {
  Clock,
  Gauge,
  Hash,
  Mail,
  MapPin,
  Phone,
  Route,
  Smartphone,
  Truck,
  type LucideIcon,
} from 'lucide-react';
import type {
  CommunicationsWorkspace,
  Correspondent,
  Elsewhere,
  Profile,
  Today,
} from '../../api/index.js';
import { useProfile } from '../../api/client.js';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { clock } from '../shared/format.js';
import { ElsewhereChip } from './ElsewhereChips.js';
import { goesElsewhere } from './elsewhere.js';
import { Face } from '../shared/Face.js';
import { Panel } from '../shared/Panel.js';
import { roleText } from '../shared/format.js';
import { PaceMark, Stops } from '../todays-routes/RouteFacts.js';
import { WorkspaceMark } from '../slack/WorkspaceMark.js';
import './profile.css';

// The profile beside a conversation, while whom it's with is working: who they are, their route
// today and how to reach them, over the conversation until its X, Escape, or a press anywhere else
// closes it; or, on a wide page (`docked`), a column of its own beside it, covering nothing, until
// its X or Escape does. It goes to each of their conversations; by their work conversation, it
// shows how far their route has come and the work phone they hold as the conversation's head does,
// live. Their phone numbers, their default first, which Call calls. What nobody knows of them is
// left out, as is a section left with nothing.

export function Profile({
  id,
  conversation,
  person,
  workspace,
  elsewhere,
  today,
  go,
  timeZone,
  docked,
  leaving,
  still,
  close,
  gone,
}: {
  /** The panel's own ID, which its openers control. */
  id: string;
  conversation: string;
  person: Correspondent;
  /** The workspace the conversation is in, for its mark. */
  workspace?: CommunicationsWorkspace;
  /** For a driver's, each of their conversations, and going to one. */
  elsewhere: readonly Elsewhere[];
  /** Their route today, as the conversation has it, live. */
  today: Today | null;
  go: (elsewhere: Elsewhere) => void;
  timeZone: string;
  /** Whether it's a column of its own beside the conversation. */
  docked: boolean;
  /** Whether it's closed, sliding away. */
  leaving: boolean;
  /** Whether it's kept open as the conversation opened, there without sliding in. */
  still: boolean;
  /** Closes it: `refocus`, focus goes back to what opened it. */
  close: (refocus: boolean) => void;
  /** Says it has slid away. */
  gone: () => void;
}) {
  const { data: profile, error } = useProfile(conversation);
  const title = useId();
  return (
    <Panel
      id={id}
      labelledBy={title}
      className="profile"
      docked={docked}
      leaving={leaving}
      still={still}
      close={close}
      gone={gone}
    >
      <ProfileShown
        title={title}
        person={person}
        profile={profile}
        today={today}
        error={error}
        workspace={workspace}
        timeZone={timeZone}
        ways={
          goesElsewhere(elsewhere, conversation) && (
            <div className="profile-ways">
              {elsewhere.map((each) => (
                <ElsewhereChip
                  key={each.kind}
                  elsewhere={each}
                  current={each.conversationId === conversation}
                  go={go}
                />
              ))}
            </div>
          )
        }
      />
    </Panel>
  );
}

/** What the profile shows of `person`: who they are, then what's known of them, as read. */
export function ProfileShown({
  title,
  person,
  profile,
  today,
  error,
  workspace,
  timeZone,
  ways,
}: {
  /** The ID its name goes by, which names the panel. */
  title: string;
  person: Correspondent;
  /** What's known of them, once read, or why it couldn't be. */
  profile?: Profile;
  /** Their route today, as the conversation has it. */
  today: Today | null;
  error: string;
  workspace?: CommunicationsWorkspace;
  timeZone: string;
  /** Each of their conversations, to go to, in place of Call. */
  ways?: ReactNode;
}) {
  const role = roleText(person);
  const call = profile?.phones[0];
  return (
    <>
      <div className="me">
        <Face {...person} size="profile" />
        <div>
          <h2 id={title}>{person.name}</h2>
          {role && <small>{role}</small>}
        </div>
      </div>
      {ways ||
        (call && (
          <div className="profile-buttons">
            <a className="profile-button" href={`tel:${call.replace(/[^\d+]/g, '')}`}>
              <Phone size={15} aria-hidden="true" />
              Call
            </a>
          </div>
        ))}
      <ErrorBox message={error} />
      {!profile && !error && <Loading />}
      {today && (
        <Facts
          title="Today"
          facts={[
            [Route, 'Route', today.route],
            [Truck, 'Van', today.van],
            [Clock, 'Wave', today.waveAt === null ? null : clock(today.waveAt, timeZone)],
            [
              MapPin,
              'Stops',
              today.stops === null ? null : today.stopsDone === null ? (
                today.stops
              ) : (
                <span className="profile-stops">
                  <Stops today={today} />
                </span>
              ),
            ],
            [Gauge, 'Pace', today.pace && <PaceMark pace={today.pace} />],
            [Smartphone, 'Work phone', today.phone && phoneText(today.phone)],
          ]}
        />
      )}
      {profile && (
        <Facts
          title="Contact"
          facts={[
            [Phone, 'Phone', profile.phones.length > 0 && <Phones phones={profile.phones} />],
            [Mail, 'Email', profile.email],
            [
              Hash,
              'Slack',
              profile.slack === null ? null : (
                <span className="profile-workspace">
                  {workspace && (
                    <WorkspaceMark workspace={workspace} className="workspace-mark small" />
                  )}
                  {profile.slack}
                </span>
              ),
            ],
          ]}
        />
      )}
    </>
  );
}

/** Their phone numbers, their default first, marked so while they have others. */
function Phones({ phones }: { phones: readonly string[] }) {
  return (
    <span className="profile-phones">
      {phones.map((phone, index) => (
        <span key={phone}>
          {index === 0 && phones.length > 1 && <span className="profile-default">Default</span>}
          {phoneText(phone)}
        </span>
      ))}
    </span>
  );
}

/** A section of what's known, each by its icon and label; one not known (null) is left out. */
function Facts({ title, facts }: { title: string; facts: [LucideIcon, string, ReactNode][] }) {
  const known = facts.filter(([, , value]) => value !== null && value !== false);
  if (known.length === 0) return null;
  return (
    <section>
      <h3>{title}</h3>
      <dl>
        {known.map(([Icon, label, value]) => (
          <div key={label}>
            <dt>
              <Icon size={14} aria-hidden="true" />
              {label}
            </dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
