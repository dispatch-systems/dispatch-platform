import type { DemoRequest } from '../../api/index.js';
import { deviceTimezone, time } from '../../../shell/frontend/lib/format.js';
import { DspAvatar } from '../../../shell/frontend/ui/index.js';
import { statusLabels, statuses } from './statuses.js';

/** The requests as a board: a column for each status, each request a card that opens it. */
export function Board({ all, choose }: { all: DemoRequest[]; choose: (id: string) => void }) {
  const zone = deviceTimezone();
  return (
    <div className="client-board">
      {statuses.map((status) => {
        const here = all.filter((request) => request.status === status);
        return (
          <section key={status} aria-label={statusLabels[status]}>
            <h2 className={`status-indicator client-dot ${status}`}>
              <i />
              {statusLabels[status]} <small>{here.length}</small>
            </h2>
            {here.map((request) => (
              <button className="client-card" key={request.id} onClick={() => choose(request.id)}>
                <span className="client-card-head">
                  <DspAvatar name={request.dsp} />
                  <span className="client-who">
                    <strong>{request.name}</strong>
                    <span>{request.dsp}</span>
                  </span>
                </span>
                {request.message && <span className="client-preview">{request.message}</span>}
                <small>{time(request.receivedAt, zone)}</small>
              </button>
            ))}
          </section>
        );
      })}
    </div>
  );
}
