import type { ReactNode } from 'react';
import type { DemoRequest, DemoStatus } from '../../api/index.js';
import { deviceTimezone, time } from '../../../shell/frontend/lib/format.js';
import { DspAvatar, Empty } from '../../../shell/frontend/ui/index.js';
import { Status } from './Status.js';
import { statusLabels, statuses } from './statuses.js';

/** Which requests the list shows: all of them, or those at one status. */
export type Showing = DemoStatus | 'all';

/**
 * The requests as an inbox: those at the status chosen in a list, the newest first, and beside it
 * whatever `open` draws, the request open on a wide screen. A phone's opens over the page.
 */
export function Inbox({
  all,
  showing,
  show,
  chosen,
  choose,
  open,
}: {
  all: DemoRequest[];
  showing: Showing;
  show: (showing: Showing) => void;
  chosen?: DemoRequest;
  choose: (id: string) => void;
  open?: ReactNode;
}) {
  const zone = deviceTimezone();
  const rows = all.filter((request) => showing === 'all' || request.status === showing);
  return (
    <div className="client-inbox">
      <div className="client-list">
        <div className="client-counts" role="group" aria-label="Requests by status">
          <button aria-pressed={showing === 'all'} onClick={() => show('all')}>
            <strong>{all.length}</strong> All
          </button>
          {statuses.map((status) => (
            <button
              className={status}
              key={status}
              aria-pressed={showing === status}
              onClick={() => show(status)}
            >
              <i />
              <strong>{all.filter((request) => request.status === status).length}</strong>{' '}
              {statusLabels[status]}
            </button>
          ))}
        </div>
        {rows.length ? (
          <ul>
            {rows.map((request) => (
              <li key={request.id}>
                <button
                  className={request.status === 'new' ? 'unread' : undefined}
                  aria-current={chosen?.id === request.id}
                  onClick={() => choose(request.id)}
                >
                  <DspAvatar name={request.dsp} />
                  <span className="client-row">
                    <span className="client-line">
                      <strong>{request.name}</strong>
                      <small>{time(request.receivedAt, zone)}</small>
                    </span>
                    <span className="client-line">
                      <span>{request.dsp}</span>
                      <Status status={request.status} />
                    </span>
                    <span className="client-preview">{request.message}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <Empty title="No requests here" />
        )}
      </div>
      {open}
    </div>
  );
}
