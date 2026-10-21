import { useEffect, useId, useRef, type ReactNode } from 'react';
import { Clock, Hash, Mail, MapPin, Phone, Route, Truck, X, type LucideIcon } from 'lucide-react';
import type { Correspondent } from '../../api/index.js';
import { useProfile } from '../../api/client.js';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { clock } from '../format.js';
import { Face } from './Face.js';
import { roles } from './parts.js';

// The profile beside a conversation: the member it's with, their route today and how to reach
// them, over the conversation until its X, Escape, or a press anywhere else closes it; or, on a
// wide page (`docked`), a column of its own beside it, covering nothing, until its X or Escape
// does. What nobody knows of them is left out, as is a section left with nothing.

export function Profile({
  id,
  conversation,
  person,
  timeZone,
  docked,
  close,
}: {
  /** The panel's own ID, which its openers control. */
  id: string;
  conversation: string;
  person: Correspondent;
  timeZone: string;
  /** Whether it's a column of its own beside the conversation. */
  docked: boolean;
  /** Closes it: `refocus`, focus goes back to what opened it. */
  close: (refocus: boolean) => void;
}) {
  const { data: profile, error } = useProfile(conversation);
  const panel = useRef<HTMLElement>(null);
  const closer = useRef<HTMLButtonElement>(null);
  const title = useId();
  // Focus moves into it as it opens.
  useEffect(() => closer.current?.focus(), []);
  useEffect(() => {
    if (docked) return;
    // A press on what opens it is that control's own: it closes it.
    const pressed = ({ target }: PointerEvent) => {
      const opener = target instanceof Element && target.closest('[data-opens-profile]');
      if (!panel.current?.contains(target as Node) && !opener) close(false);
    };
    document.addEventListener('pointerdown', pressed);
    return () => document.removeEventListener('pointerdown', pressed);
  }, [docked, close]);
  const today = profile?.today;
  return (
    <aside
      ref={panel}
      id={id}
      className="profile"
      role="dialog"
      aria-labelledby={title}
      onKeyDown={(event) => {
        if (event.key === 'Escape') close(true);
      }}
    >
      <button
        ref={closer}
        className="icon-button close"
        aria-label="Close"
        onClick={() => close(true)}
      >
        <X size={16} />
      </button>
      <div className="me">
        <Face {...person} size="profile" />
        <div>
          <h2 id={title}>{person.name}</h2>
          {person.role && <small>{roles[person.role]}</small>}
        </div>
      </div>
      {profile?.phone && (
        <div className="profile-buttons">
          <a className="profile-button" href={`tel:${profile.phone.replace(/[^\d+]/g, '')}`}>
            <Phone size={15} aria-hidden="true" />
            Call
          </a>
        </div>
      )}
      <ErrorBox message={error} />
      {!profile && !error && <Loading />}
      {today && (
        <Facts
          title="Today"
          facts={[
            [Route, 'Route', today.route],
            [Truck, 'Van', today.van],
            [Clock, 'Wave', today.waveAt === null ? null : clock(today.waveAt, timeZone)],
            [MapPin, 'Stops', today.stops],
          ]}
        />
      )}
      {profile && (
        <Facts
          title="Contact"
          facts={[
            [Phone, 'Phone', profile.phone === null ? null : phoneText(profile.phone)],
            [Mail, 'Email', profile.email],
            [Hash, 'Slack', profile.slack],
          ]}
        />
      )}
    </aside>
  );
}

/** A section of what's known, each by its icon and label; one not known (null) is left out. */
function Facts({ title, facts }: { title: string; facts: [LucideIcon, string, ReactNode][] }) {
  const known = facts.filter(([, , value]) => value !== null);
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
