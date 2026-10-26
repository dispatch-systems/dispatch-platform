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
} from '../../api/index.js';
import { useProfile } from '../../api/client.js';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { clock, lastFour } from '../format.js';
import { ElsewhereChip } from './Elsewhere.js';
import { Face } from './Face.js';
import { Panel } from './Panel.js';
import { roleText } from './parts.js';
import { PaceMark, Stops } from './RouteFacts.js';
import { WorkspaceMark } from './WorkspaceMark.js';
import './profile.css';

// The profile beside a conversation: whom it's with, a member or a driver, their route today and
// how to reach them, over the conversation until its X, Escape, or a press anywhere else closes
// it; or, on a wide page (`docked`), a column of its own beside it, covering nothing, until its X
// or Escape does. A driver's, by their work conversation, goes to each of their conversations, and
// shows how far their route has come and the work phone they hold. What nobody knows of them is
// left out, as is a section left with nothing.

export function Profile({
  id,
  conversation,
  person,
  workspace,
  elsewhere,
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
        error={error}
        workspace={workspace}
        timeZone={timeZone}
        ways={
          elsewhere.length > 0 && (
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
  error: string;
  workspace?: CommunicationsWorkspace;
  timeZone: string;
  /** Each of their conversations, to go to, in place of Call. */
  ways?: ReactNode;
}) {
  const today = profile?.today;
  const role = roleText(person);
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
        (profile?.phone && (
          <div className="profile-buttons">
            <a className="profile-button" href={`tel:${profile.phone.replace(/[^\d+]/g, '')}`}>
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
            [
              Smartphone,
              'Work phone',
              today.phone && (
                <span className="profile-phone">
                  {lastFour(today.phone)}
                  {today.phoneSince !== null && (
                    <small>Since {clock(today.phoneSince, timeZone)}</small>
                  )}
                </span>
              ),
            ],
          ]}
        />
      )}
      {profile && (
        <Facts
          title="Contact"
          facts={[
            [Phone, 'Phone', profile.phone === null ? null : phoneText(profile.phone)],
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
