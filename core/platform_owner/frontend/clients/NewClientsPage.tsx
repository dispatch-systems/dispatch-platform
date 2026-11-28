import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Columns3, List, Mail } from 'lucide-react';
import type { DemoRequest, DemoStatus } from '../../api/index.js';
import { setDemoRequestStatus, useDemoRequests } from '../../api/client.js';
import { deviceTimezone, time } from '../../../shell/frontend/lib/format.js';
import { useUpdateState } from '../../../shell/frontend/runtime/navigation-state.js';
import {
  hashQuery,
  navigate,
  parseHash,
  platformHash,
} from '../../../shell/frontend/runtime/navigation.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import {
  DataState,
  Empty,
  ErrorBox,
  Header,
  Modal,
  ViewSwitch,
  type ViewOption,
} from '../../../shell/frontend/ui/index.js';
import { statusLabels, statuses } from './statuses.js';

// The requests for a demo the public site's form sent, for the owner to answer and keep track
// of: an inbox, the list beside the open request, or a board with a column for each status.
type View = 'inbox' | 'board';
/** Where this browser keeps the view the owner last chose. */
const VIEW = 'new-clients.view';
const views: ViewOption<View>[] = [
  { value: 'inbox', label: 'Inbox', icon: List },
  { value: 'board', label: 'Board', icon: Columns3 },
];
const none: DemoRequest[] = [];
// A phone's width, where a request opens over the list, not beside it.
const narrow = '(max-width: 760px)';
const watchNarrow = (changed: () => void) => {
  const query = matchMedia(narrow);
  query.addEventListener('change', changed);
  return () => query.removeEventListener('change', changed);
};
const useNarrow = () => useSyncExternalStore(watchNarrow, () => matchMedia(narrow).matches);

type SetStatus = (request: DemoRequest, status: DemoStatus) => void;

// A link that writes to `email`. The address is a visitor's own text, and a valid one may hold
// `?` or `#`: encoded, nothing in it ends the address to add a subject or someone to copy.
const mailto = (email: string) => `mailto:${encodeURIComponent(email)}`;

const Status = ({ status }: { status: DemoStatus }) => (
  <span className={`status-indicator client-dot ${status}`}>
    <i />
    {statusLabels[status]}
  </span>
);

// One request in full: who asked, for which DSP, what they wrote and where it stands.
function Request({ request, set }: { request: DemoRequest; set: SetStatus }) {
  return (
    <div className="client-request">
      <dl className="detail-list">
        <div>
          <dt>DSP</dt>
          <dd>{request.dsp}</dd>
        </div>
        <div>
          <dt>Email</dt>
          <dd>
            <a href={mailto(request.email)}>{request.email}</a>
          </dd>
        </div>
        <div>
          <dt>Received</dt>
          <dd>{time(request.receivedAt, deviceTimezone())}</dd>
        </div>
        <div>
          <dt>Status</dt>
          <dd>
            <select
              aria-label="Status"
              value={request.status}
              onChange={(event) => set(request, event.target.value as DemoStatus)}
            >
              {statuses.map((status) => (
                <option key={status} value={status}>
                  {statusLabels[status]}
                </option>
              ))}
            </select>
          </dd>
        </div>
      </dl>
      {request.message && <p className="client-message">{request.message}</p>}
      <div className="form-actions">
        <a className="link-button client-reply" href={mailto(request.email)}>
          <Mail size={16} />
          Reply by email
        </a>
      </div>
    </div>
  );
}

function Counts({
  all,
  showing,
  show,
}: {
  all: DemoRequest[];
  showing: DemoStatus | 'all';
  show: (status: DemoStatus | 'all') => void;
}) {
  return (
    <div className="client-counts" role="group" aria-label="Requests by status">
      <button aria-pressed={showing === 'all'} onClick={() => show('all')}>
        <strong>{all.length}</strong> Requests
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
  );
}

function Inbox({
  rows,
  set,
  chosen,
  open,
}: {
  rows: DemoRequest[];
  set: SetStatus;
  chosen?: DemoRequest;
  open: (id: string) => void;
}) {
  const zone = deviceTimezone();
  return (
    <div className="client-inbox">
      {rows.length ? (
        <ul className="client-list">
          {rows.map((request) => (
            <li key={request.id}>
              <button
                className={request.status === 'new' ? 'unread' : undefined}
                aria-current={chosen?.id === request.id}
                onClick={() => open(request.id)}
              >
                <span className="client-line">
                  <strong>{request.name}</strong>
                  <small>{time(request.receivedAt, zone)}</small>
                </span>
                <span className="client-line">
                  <span>{request.dsp}</span>
                  <Status status={request.status} />
                </span>
                <span className="client-preview">{request.message}</span>
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <Empty title="No requests here" />
      )}
      {chosen && (
        <section className="client-open" aria-label={chosen.name}>
          <h2>{chosen.name}</h2>
          <Request request={chosen} set={set} />
        </section>
      )}
    </div>
  );
}

function Board({ rows, open }: { rows: DemoRequest[]; open: (id: string) => void }) {
  const zone = deviceTimezone();
  return (
    <div className="client-board">
      {statuses.map((status) => {
        const here = rows.filter((request) => request.status === status);
        return (
          <section key={status} aria-label={statusLabels[status]}>
            <h2 className={`status-indicator client-dot ${status}`}>
              <i />
              {statusLabels[status]} <small>{here.length}</small>
            </h2>
            {here.map((request) => (
              <button className="client-card" key={request.id} onClick={() => open(request.id)}>
                <strong>{request.name}</strong>
                <span>{request.dsp}</span>
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

export function NewClientsPage() {
  const { data, error, refresh } = useDemoRequests(60_000);
  const phone = useNarrow();
  const [view, setView] = useState<View>(() =>
      localStorage.getItem(VIEW) === 'board' ? 'board' : 'inbox',
    ),
    [showing, show] = useUpdateState<DemoStatus | 'all'>('clients-status', 'all'),
    // The open request rides in the address, so a reload, a link and Back keep it.
    [chosenId, setChosen] = useState(() => hashQuery().get('request') ?? ''),
    // A status the owner just chose shows at once, until the server answers that choice: with
    // every request as it holds them, or a failure that leaves this one where it stood.
    [saving, setSaving] = useState<Record<string, { status: DemoStatus; turn: number }>>({});
  const turns = useRef(0);
  useEffect(() => {
    const followed = () => {
      if (parseHash(location.hash).page === 'new-clients')
        setChosen(hashQuery().get('request') ?? '');
    };
    addEventListener('hashchange', followed);
    return () => removeEventListener('hashchange', followed);
  }, []);
  const all = (data ?? none).map((request) =>
    saving[request.id] ? { ...request, status: saving[request.id]!.status } : request,
  );
  const open = (id: string) => {
    setChosen(id);
    navigate(platformHash('new-clients', id ? { request: id } : undefined));
  };
  const change = useAction(async (request: DemoRequest, status: DemoStatus) => {
    const turn = ++turns.current;
    setSaving((before) => ({ ...before, [request.id]: { status, turn } }));
    try {
      await setDemoRequestStatus(request.id, status);
    } finally {
      // A choice made for the same request since stays shown until its own answer.
      setSaving((before) => {
        if (before[request.id]?.turn !== turn) return before;
        const { [request.id]: _, ...rest } = before;
        return rest;
      });
    }
  });
  // A view opens with no request over it: the one open beside the inbox's list isn't the board's.
  const switchTo = (to: View) => {
    setView(to);
    localStorage.setItem(VIEW, to);
    if (chosenId) open('');
  };
  const set: SetStatus = (request, status) => void change.run(request, status);
  const rows = all.filter((request) => showing === 'all' || request.status === showing);
  // A wide inbox always has a request open beside its list: the newest shown, until another is
  // chosen. A phone's inbox and the board open one over the page.
  const beside = view === 'inbox' && !phone;
  const chosen = all.find((request) => request.id === chosenId) ?? (beside ? rows[0] : undefined);
  return (
    <>
      <Header title="New Clients">
        {all.length > 0 && <ViewSwitch value={view} options={views} onChange={switchTo} />}
      </Header>
      <ErrorBox message={error} />
      <DataState data={data} failed={Boolean(error)} retry={refresh}>
        {() =>
          !all.length ? (
            <Empty title="No requests yet" />
          ) : view === 'inbox' ? (
            <>
              <Counts all={all} showing={showing} show={show} />
              <Inbox rows={rows} set={set} chosen={chosen} open={open} />
            </>
          ) : (
            <Board rows={all} open={open} />
          )
        }
      </DataState>
      {chosen && !beside && (
        <Modal title={chosen.name} variant="sheet" onClose={() => open('')}>
          <Request request={chosen} set={set} />
        </Modal>
      )}
    </>
  );
}
