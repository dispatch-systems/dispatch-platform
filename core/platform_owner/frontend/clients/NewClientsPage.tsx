import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Columns3, List } from 'lucide-react';
import type { DemoRequest, DemoStatus } from '../../api/index.js';
import { deleteDemoRequest, setDemoRequestStatus, useDemoRequests } from '../../api/client.js';
import { useFillsWindow } from '../../../shell/frontend/runtime/fills.js';
import { useUpdateState } from '../../../shell/frontend/runtime/navigation-state.js';
import {
  hashQuery,
  navigate,
  parseHash,
  platformHash,
} from '../../../shell/frontend/runtime/navigation.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import {
  ConfirmDialog,
  DataState,
  Empty,
  ErrorBox,
  Header,
  Modal,
  ViewSwitch,
  type ViewOption,
} from '../../../shell/frontend/ui/index.js';
import { Board } from './Board.js';
import { Inbox, type Showing } from './Inbox.js';
import { Request } from './Request.js';

// The requests for a demo the public site's form sent, for the owner to answer and keep track
// of: an inbox, the list beside the open request, or a board with a column for each status.
type View = 'inbox' | 'board';
const views: ViewOption<View>[] = [
  { value: 'inbox', label: 'Inbox', icon: List },
  { value: 'board', label: 'Board', icon: Columns3 },
];
/** Where this browser keeps the view the owner last chose. */
const VIEW = 'new-clients.view';
const none: DemoRequest[] = [];
// A phone's width, where the page scrolls and a request opens over the list, not beside it.
// Wider, the page fills the window and its list and board scroll within it.
const narrow = '(max-width: 760px)';
const wide = '(min-width: 761px)';
const watchNarrow = (changed: () => void) => {
  const query = matchMedia(narrow);
  query.addEventListener('change', changed);
  return () => query.removeEventListener('change', changed);
};
const useNarrow = () => useSyncExternalStore(watchNarrow, () => matchMedia(narrow).matches);

export function NewClientsPage() {
  const { data, error, refresh } = useDemoRequests(60_000);
  const phone = useNarrow();
  useFillsWindow(wide);
  const [view, setView] = useState<View>(() =>
      localStorage.getItem(VIEW) === 'board' ? 'board' : 'inbox',
    ),
    [showing, show] = useUpdateState<Showing>('clients-status', 'all'),
    // The open request rides in the address, so a reload, a link and Back keep it.
    [chosenId, setChosen] = useState(() => hashQuery().get('request') ?? ''),
    // A status the owner just chose shows at once, until the server answers that choice: with
    // every request as it holds them, or a failure that leaves this one where it stood.
    [saving, setSaving] = useState<Record<string, { status: DemoStatus; turn: number }>>({}),
    [deleting, setDeleting] = useState<DemoRequest>();
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
  const choose = (id: string) => {
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
  const remove = useAction(
    async (request: DemoRequest) => {
      await deleteDemoRequest(request.id);
      setDeleting(undefined);
      if (request.id === chosenId) choose('');
    },
    { success: 'Request deleted' },
  );
  // A view opens with no request over it: the one open beside the inbox's list isn't the board's.
  const switchTo = (to: View) => {
    setView(to);
    localStorage.setItem(VIEW, to);
    if (chosenId) choose('');
  };
  // A wide inbox always has a request open beside its list: the newest shown, until another is
  // chosen. A phone's inbox and the board open one over the page.
  const beside = view === 'inbox' && !phone;
  const shown = all.filter((request) => showing === 'all' || request.status === showing);
  const chosen = all.find((request) => request.id === chosenId) ?? (beside ? shown[0] : undefined);
  const request = (titled: boolean) =>
    chosen && (
      <Request
        request={chosen}
        titled={titled}
        setStatus={(of, status) => void change.run(of, status)}
        remove={setDeleting}
      />
    );
  return (
    <div className="clients-page">
      <Header title="New Clients">
        {all.length > 0 && <ViewSwitch value={view} options={views} onChange={switchTo} />}
      </Header>
      <ErrorBox message={error} />
      <DataState data={data} failed={Boolean(error)} retry={refresh}>
        {() =>
          !all.length ? (
            <Empty title="No requests yet" />
          ) : view === 'inbox' ? (
            <Inbox
              all={all}
              showing={showing}
              show={show}
              chosen={chosen}
              choose={choose}
              open={
                beside &&
                chosen && (
                  <section className="client-open" aria-label={chosen.name}>
                    {request(true)}
                  </section>
                )
              }
            />
          ) : (
            <Board all={all} choose={choose} />
          )
        }
      </DataState>
      {chosen && !beside && (
        <Modal title={chosen.name} variant="sheet" onClose={() => choose('')}>
          {request(false)}
        </Modal>
      )}
      {deleting && (
        <ConfirmDialog
          title="Delete this request?"
          confirm="Delete"
          tone="danger"
          busy={remove.busy}
          onConfirm={() => void remove.run(deleting)}
          onCancel={() => setDeleting(undefined)}
        >
          {deleting.name}, {deleting.dsp}
        </ConfirmDialog>
      )}
    </div>
  );
}
