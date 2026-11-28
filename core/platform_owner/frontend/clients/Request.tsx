import { Mail, Trash2 } from 'lucide-react';
import type { DemoRequest, DemoStatus } from '../../api/index.js';
import { deviceTimezone, time } from '../../../shell/frontend/lib/format.js';
import { DspAvatar } from '../../../shell/frontend/ui/index.js';
import { statusLabels, statuses } from './statuses.js';

// A link that writes to `email`. The address is a visitor's own text, and a valid one may hold
// `?` or `#`: encoded, nothing in it ends the address to add a subject or someone to copy.
const mailto = (email: string) => `mailto:${encodeURIComponent(email)}`;

/**
 * One request in full: who asked, for which DSP and where it stands, what they wrote, and what
 * the owner does with it. `titled` beside the inbox's list; a sheet names whose it is itself.
 */
export function Request({
  request,
  titled = false,
  setStatus,
  remove,
}: {
  request: DemoRequest;
  titled?: boolean;
  setStatus: (request: DemoRequest, status: DemoStatus) => void;
  /** Asks to delete it; the page confirms first. */
  remove: (request: DemoRequest) => void;
}) {
  return (
    <div className="client-request">
      <header>
        <DspAvatar name={request.dsp} />
        <div className="client-who">
          {titled && <h2>{request.name}</h2>}
          <span>
            {request.dsp} · <a href={mailto(request.email)}>{request.email}</a>
          </span>
        </div>
        <select
          aria-label="Status"
          value={request.status}
          onChange={(event) => setStatus(request, event.target.value as DemoStatus)}
        >
          {statuses.map((status) => (
            <option key={status} value={status}>
              {statusLabels[status]}
            </option>
          ))}
        </select>
      </header>
      <div className="client-body">
        <small>{time(request.receivedAt, deviceTimezone())}</small>
        {request.message ? <p>{request.message}</p> : <p className="client-none">No message</p>}
      </div>
      <footer>
        <a className="link-button client-reply" href={mailto(request.email)}>
          <Mail size={16} />
          Reply by email
        </a>
        <button className="client-delete" onClick={() => remove(request)}>
          <Trash2 size={15} />
          Delete
        </button>
      </footer>
    </div>
  );
}
