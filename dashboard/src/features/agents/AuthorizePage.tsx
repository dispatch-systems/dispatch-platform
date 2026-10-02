import { useEffect, useState, type ReactNode } from 'react';
import { Globe, Info, TriangleAlert } from 'lucide-react';
import type {
  AgentDsp,
  OAuthApproval,
  OAuthReplaced,
  OAuthRequest,
} from '../../../../shared/contracts/index.js';
import { ApiError } from '../../app/api.js';
import {
  approveOAuthRequest,
  denyOAuthRequest,
  readOAuthRequest,
  useAgentKeys,
  useOAuthRequest,
} from '../../app/endpoints.js';
import { hashQuery, platformHash } from '../../app/navigation.js';
import { useAction } from '../../app/useAction.js';
import { appKindName, blankKey } from '../../lib/agents.js';
import { calendarDay } from '../../lib/format.js';
import { Badge, DataState, DetailList, ErrorBox, Header } from '../../ui/index.js';
import { DspReach, LocationsSwitch, ToolChoice } from './KeyChoices.js';

const again = 'Start the connection again from your app.';
const expired = `This request expired. ${again}`;
const connectTab = (
  <a className="underlined-link" href={platformHash('agents', { tab: 'connect' })}>
    Agents → Connect
  </a>
);
/** Why Dispatch turned a request away before asking: `#authorize?error=<code>`, with the
 * kind of app as `app=<id>` when it is one the owner turned off. */
function refusal(error: string, app: string | null): ReactNode {
  switch (error) {
    case 'unknown_app':
      return 'Dispatch doesn’t accept this app.';
    // Opened only from Dispatch itself, never from a link an app or anyone else sent.
    case 'pairing_closed':
      return (
        <>
          Connecting is closed. Open {connectTab}, copy your app’s command or choose Allow
          connecting, then start again from your app.
        </>
      );
    case 'app_not_allowed':
      return (
        <>
          Dispatch doesn’t accept {appKindName(app)} yet. Turn it on under {connectTab}.
        </>
      );
    case 'app_unavailable':
      return 'Dispatch couldn’t check this app right now. Try again in a few minutes.';
    case 'invalid_redirect':
      return `This app asked to send access to an address it never registered. ${again}`;
    case 'rate_limited':
      return `Too many connection attempts. Wait a few minutes, then ${again.toLowerCase()}`;
    default:
      return `This connection can’t go ahead. ${again}`;
  }
}

/** Platform → Agents → an app asking to connect, opened from `#authorize?request=<id>`. */
export function AuthorizePage() {
  const [query, setQuery] = useState(() => hashQuery().toString());
  // Another request opened in this tab changes only the address's query.
  useEffect(() => {
    const changed = () => setQuery(hashQuery().toString());
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, []);
  return (
    <>
      <Header title="Connect an app" />
      <section className="agents-authorize">
        <Authorization key={query} query={new URLSearchParams(query)} />
      </section>
    </>
  );
}

function Authorization({ query }: { query: URLSearchParams }) {
  const id = query.get('request') ?? '';
  const error = query.get('error');
  const request = useOAuthRequest(error ? '' : id);
  const agents = useAgentKeys();
  const [gone, setGone] = useState(false);
  const problem: ReactNode = error
    ? refusal(error, query.get('app'))
    : !id
      ? `This link is incomplete. ${again}`
      : gone || request.errorCode === 'authorization_not_found'
        ? expired
        : '';
  if (problem)
    return (
      <p className="notice" role="status">
        {problem}
      </p>
    );
  const loaded = request.data && agents.data && { pending: request.data, dsps: agents.data.dsps };
  return (
    <DataState
      data={loaded || undefined}
      error={request.error || agents.error}
      retry={() => {
        request.refresh();
        agents.refresh();
      }}
    >
      {({ pending, dsps }) => (
        <Approval request={pending} dsps={dsps} expire={() => setGone(true)} />
      )}
    </DataState>
  );
}

function Approval({
  request,
  dsps,
  expire,
}: {
  request: OAuthRequest;
  dsps: AgentDsp[];
  expire: () => void;
}) {
  const { app } = request;
  const [form, setForm] = useState<OAuthApproval>(() => {
    const { allDsps, dsps, tools, locations } = blankKey();
    return { name: app.name, allDsps, dsps, tools, locations };
  });
  const [leaving, setLeaving] = useState(false);
  // The connection approving would replace, as last checked for a name.
  const [checked, setChecked] = useState({ name: app.name, replaces: request.replaces });
  const set = (change: Partial<OAuthApproval>) => setForm((current) => ({ ...current, ...change }));
  const approval = { ...form, name: form.name.trim() };
  const ready = approval.name.length > 0 && (approval.allDsps || approval.dsps.length > 0);
  const replaces = checked.name === approval.name ? checked.replaces : null;
  const check = async (name: string) => {
    const { replaces } = await readOAuthRequest(request.id, name);
    setChecked({ name, replaces });
    return replaces;
  };
  const gone = (error: unknown) => {
    if (error instanceof ApiError && error.code === 'authorization_not_found') expire();
  };
  // A changed name asks again once the owner pauses; approving checks again regardless.
  useEffect(() => {
    if (!approval.name || approval.name === checked.name) return;
    const timer = setTimeout(() => void check(approval.name).catch(gone), 400);
    return () => clearTimeout(timer);
  }, [approval.name, checked.name]);
  // Either answer sends the browser back to the app, which reads it from the address.
  const answer = useAction(
    async (approve: boolean) => {
      try {
        // The owner approves only a replacement they were shown.
        if (approve && !same(await check(approval.name), replaces)) return;
        const { redirect } = approve
          ? await approveOAuthRequest(request.id, approval)
          : await denyOAuthRequest(request.id);
        setLeaving(true);
        window.location.assign(redirect);
      } catch (error) {
        gone(error);
        throw error;
      }
    },
    { inline: true },
  );
  const busy = answer.busy || leaving;
  // An app Dispatch doesn't know, on a website: where access goes is what matters most.
  const website = !app.verified && !app.redirectScheme && app.redirectHost !== 'this computer';
  const name = <bdi>{app.name}</bdi>;
  return (
    <form
      className="agents-form"
      aria-labelledby="agents-authorize-app"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) void answer.run(true);
      }}
    >
      <div className="agents-authorize-app">
        <h2 id="agents-authorize-app">{name}</h2>
        <Badge value={app.verified ? 'verified' : 'unverified'} />
      </div>
      {website && (
        <p className="agents-sends">
          <Globe size={18} aria-hidden="true" />
          <span>
            Sends access to: <strong>{app.redirectHost}</strong>
          </span>
        </p>
      )}
      <DetailList
        items={[
          ...(website
            ? []
            : ([
                [
                  'Sends access to',
                  app.redirectScheme
                    ? `an app on ${app.redirectHost} (${app.redirectScheme}://…)`
                    : app.redirectHost,
                ],
              ] as [string, ReactNode][])),
          ['Access', 'Read only'],
        ]}
      />
      <label>
        Connection name
        <input
          name="name"
          required
          maxLength={80}
          value={form.name}
          onChange={(event) => set({ name: event.target.value })}
        />
      </label>
      <DspReach dsps={dsps} form={form} set={set} />
      <ToolChoice form={form} set={set} />
      <LocationsSwitch form={form} set={set} />
      {replaces && (
        <div className="notice agents-notice" role="status">
          <TriangleAlert size={16} aria-hidden="true" />
          <span>
            Approving replaces “<bdi>{replaces.name}</bdi>”, connected{' '}
            {calendarDay(replaces.connectedAt)}. Its current connection stops working.
          </span>
        </div>
      )}
      <div className="notice agents-consent">
        <Info size={16} aria-hidden="true" />
        <span>
          {!app.verified && <>Unverified app: it says it is “{name}”. </>}
          Only approve if you started connecting {name} yourself just now. If you didn’t, choose
          Deny.
          {app.redirectHost === 'this computer' &&
            ' Access will be sent to an app running on this computer.'}
        </span>
      </div>
      <ErrorBox message={answer.error} />
      {leaving && <p role="status">Sending you back to {name}…</p>}
      <div className="form-actions">
        <button type="button" disabled={busy} onClick={() => void answer.run(false)}>
          Deny
        </button>
        <button className="primary" disabled={!ready || busy}>
          Approve
        </button>
      </div>
    </form>
  );
}

const same = (a: OAuthReplaced | null, b: OAuthReplaced | null) =>
  a?.name === b?.name && a?.connectedAt === b?.connectedAt;
