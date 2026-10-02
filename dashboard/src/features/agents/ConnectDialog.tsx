import { Fragment, useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { ArrowRight, Check, ChevronRight, Clock, LoaderCircle } from 'lucide-react';
import type { AgentDsp, AgentKey } from '../../../../shared/contracts/index.js';
import { useAgentKeys } from '../../app/endpoints.js';
import { platformHash } from '../../app/navigation.js';
import {
  connectApps,
  connectedAs,
  inUse,
  reachText,
  signIns,
  toolLabels,
  type ConnectApp,
} from '../../lib/agents.js';
import { countdown } from '../../lib/format.js';
import { DetailList, Modal } from '../../ui/index.js';
import { AppIcon } from './AppIcon.js';
import { CopyButton } from './CopyButton.js';
import { usePairing } from './Pairing.js';

/** How often the dialog looks for the app while it may connect. */
const POLL_MS = 2500;

/** Re-renders every `ms` while `ms` is set, for what reads the clock. */
function useTick(ms: number) {
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!ms) return;
    const timer = setInterval(() => setTick((tick) => tick + 1), ms);
    return () => clearInterval(timer);
  }, [ms]);
}

/** One line to copy: a command or an address. It wraps only around the address, so a
 * command never breaks mid-phrase, and on a narrow screen between words, so a flag never breaks
 * at its hyphen; only the address may break anywhere. */
function Line({ text, label, onCopy }: { text: string; label: string; onCopy: () => void }) {
  const chunks: string[] = [];
  for (const word of text.split(' ')) {
    const last = chunks.at(-1);
    if (last === undefined || word.includes('://') || last.includes('://')) chunks.push(word);
    else chunks[chunks.length - 1] = `${last} ${word}`;
  }
  return (
    <div className="agents-key agents-line">
      <code>
        {chunks.map((chunk, index) => (
          <Fragment key={index}>
            {index > 0 && ' '}
            {chunk.includes('://') ? (
              chunk
            ) : (
              <span>
                {chunk.split(' ').map((word, at) => (
                  <Fragment key={at}>
                    {at > 0 && ' '}
                    <span>{word}</span>
                  </Fragment>
                ))}
              </span>
            )}
          </Fragment>
        ))}
      </code>
      <CopyButton text={text} label={label} onCopy={onCopy} />
    </div>
  );
}

/** Between the places to click in another app, in turn. */
const then = (
  <>
    <ChevronRight className="agents-path" size={14} aria-hidden="true" />
    <span className="sr-only"> then </span>
  </>
);

function Steps({ steps }: { steps: ReactNode[] }) {
  return (
    <ol className="agents-steps">
      {steps.map((step, index) => (
        <li key={index}>
          <span className="agents-step-number" aria-hidden="true">
            {index + 1}
          </span>
          <div>{step}</div>
        </li>
      ))}
    </ol>
  );
}

/** A way that isn't the usual one, folded away until asked for. */
function More({ summary, children }: { summary: string; children: ReactNode }) {
  return (
    <details className="agents-remote">
      <summary>
        <ChevronRight size={14} aria-hidden="true" />
        {summary}
      </summary>
      {children}
    </details>
  );
}

/** "Connect an app": which app, then its steps, then that it connected. Copying any of the
 * steps lets apps connect for ten minutes, and while they may the dialog watches for this one,
 * calling `connected` once it has. */
export function ConnectDialog({ close, connected }: { close: () => void; connected: () => void }) {
  const [app, setApp] = useState<ConnectApp | null>(null);
  const [copied, setCopied] = useState(false);
  const [done, setDone] = useState<AgentKey | null>(null);
  const pairing = usePairing();
  const steps = Boolean(app && !done);
  useTick(steps ? 1000 : 0);
  const now = Date.now();
  const open = pairing.until !== null && Date.parse(pairing.until) > now;
  // While an app may connect, the dialog looks for it among the apps connected since it opened.
  const keys = useAgentKeys(steps && open ? POLL_MS : 0);
  const [known, setKnown] = useState<Set<string>>();
  useEffect(() => {
    if (!keys.data) return;
    if (!known) {
      setKnown(new Set(keys.data.keys.map((key) => key.id)));
      return;
    }
    if (!app || done) return;
    const found = keys.data.keys.find(
      (key) => !known.has(key.id) && inUse(key) && connectedAs(key, app.id),
    );
    if (found) {
      setDone(found);
      connected();
    }
  }, [keys.data, known, app, done, connected]);
  // Each step starts at its first control: the way back, the close button, or Done.
  const pane = useRef<HTMLDivElement>(null);
  const shown = useRef('');
  useEffect(() => {
    const step = done ? 'done' : (app?.id ?? '');
    if (shown.current === step) return;
    shown.current = step;
    const dialog = pane.current?.closest('[role="dialog"]');
    dialog?.querySelector<HTMLElement>(done ? '.agents-connect-footer button' : 'button')?.focus();
  }, [app, done]);

  const copy = () => {
    setCopied(true);
    pairing.open();
  };
  const which = useId();
  const name = app?.id === 'other' ? 'another app' : app?.label;
  return (
    <Modal
      title={name ? `Connect ${name}` : 'Connect an app'}
      onClose={close}
      onBack={app && !done ? () => setApp(null) : undefined}
    >
      <div className="agents-connect" ref={pane}>
        {done && app ? (
          <Connected agentKey={done} app={app} dsps={keys.data?.dsps ?? []} close={close} />
        ) : app ? (
          <>
            <div className="agents-connect-body">
              <AppSteps app={app} onCopy={copy} />
            </div>
            <Status
              app={app}
              copied={copied}
              left={open && pairing.until ? countdown(pairing.until, now) : ''}
              busy={pairing.busy}
              error={pairing.error}
            />
          </>
        ) : (
          <div className="agents-connect-body">
            <p className="agents-connect-legend" id={which}>
              Which app?
            </p>
            <div className="agents-tiles" role="group" aria-labelledby={which}>
              {connectApps.map((choice) => (
                <button
                  key={choice.id}
                  type="button"
                  className="agents-tile"
                  onClick={() => {
                    setApp(choice);
                    setCopied(false);
                  }}
                >
                  <AppIcon app={choice.id} large />
                  <span>{choice.label}</span>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}

/** The steps that connect `app`: a terminal app's command, ChatGPT's form, or any other app's
 * address. */
function AppSteps({ app, onCopy }: { app: ConnectApp; onCopy: () => void }) {
  const { mcp, next, terminals, bridge } = signIns(window.location.origin);
  const terminal = terminals.find((candidate) => candidate.id === app.id);
  if (terminal)
    return (
      <>
        <Steps
          steps={[
            <>
              Copy this command
              <Line
                text={terminal.command}
                label={`Copy ${terminal.label} command`}
                onCopy={onCopy}
              />
            </>,
            'Run it in your terminal',
            terminal.next,
          ]}
        />
        <div className="agents-more">
          <More summary="Using another computer?">
            <p>{terminal.remote.lead}</p>
            {terminal.remote.command && (
              <Line
                text={terminal.remote.command}
                label={`Copy ${terminal.label} remote command`}
                onCopy={onCopy}
              />
            )}
            <p>
              Open the link in any browser and approve. Then copy the address the browser ends on,
              even if the page doesn’t load, and paste it into the terminal.
            </p>
          </More>
          <More summary={`Let ${terminal.label} set itself up instead (copy a prompt)`}>
            <pre className="agents-prompt">{terminal.prompt}</pre>
            <CopyButton
              text={terminal.prompt}
              label={`Copy prompt for ${terminal.label}`}
              onCopy={onCopy}
            >
              Copy prompt
            </CopyButton>
          </More>
        </div>
      </>
    );
  if (app.id === 'chatgpt')
    return (
      <Steps
        steps={[
          <>
            On{' '}
            <a href="https://chatgpt.com" target="_blank" rel="noopener">
              chatgpt.com
            </a>
            , open <kbd>Plugins</kbd>
            {then}
            <kbd>Add</kbd>
            {then}
            <kbd>Create custom MCP server</kbd>
            <p className="agents-step-note">Needs ChatGPT Plus or higher.</p>
          </>,
          <>
            Fill in the form, then click <kbd>Create</kbd>
            <DetailList
              className="agents-fields"
              items={[
                ['Name', 'Dispatch'],
                ['URL', <Line text={mcp} label="Copy MCP address" onCopy={onCopy} />],
                ['Authentication', 'OAuth'],
              ]}
            />
          </>,
          next,
        ]}
      />
    );
  return (
    <>
      <Steps
        steps={[
          <>
            Add Dispatch as an MCP server in your app with this address
            <Line text={mcp} label="Copy MCP address" onCopy={onCopy} />
          </>,
          'Your app opens Dispatch to sign in — approve it there',
        ]}
      />
      <div className="agents-aside">
        <p>
          Only runs local servers? Use <code>{bridge}</code>
          <CopyButton text={bridge} label="Copy local server command" onCopy={onCopy} />
        </p>
        <p>
          Can’t sign in?{' '}
          <a className="agents-link" href={platformHash('agents', { tab: 'keys' })}>
            Use a key instead
            <ArrowRight size={14} aria-hidden="true" />
          </a>
        </p>
      </div>
    </>
  );
}

/** What the dialog is waiting for, and for how much longer: `left` while apps may connect,
 * `busy` while it lets them. */
function Status({
  app,
  copied,
  left,
  busy,
  error,
}: {
  app: ConnectApp;
  copied: boolean;
  left: string;
  busy: boolean;
  error: string;
}) {
  const thing = app.id === 'chatgpt' ? 'URL' : app.id === 'other' ? 'address' : 'command';
  const who = app.id === 'other' ? 'your app' : app.label;
  const waiting = !error && copied && Boolean(left || busy);
  const ran = !error && copied && !waiting;
  return (
    <div className={`agents-connect-status${error ? ' failed' : ''}`}>
      {waiting && <LoaderCircle className="spin" size={16} aria-hidden="true" />}
      {ran && <Clock size={16} aria-hidden="true" />}
      <span role="status">
        {error ||
          (!copied
            ? `Copy the ${thing} to start.`
            : waiting
              ? `Waiting for ${who} to connect…`
              : `Time ran out. Copy the ${thing} again.`)}
      </span>
      {/* Outside the live line, so the seconds going by aren't read out. */}
      {waiting && left && <span className="agents-left">({left} left)</span>}
    </div>
  );
}

/** The app connected: what it reaches, and how to start using it. */
function Connected({
  agentKey,
  app,
  dsps,
  close,
}: {
  agentKey: AgentKey;
  app: ConnectApp;
  dsps: AgentDsp[];
  close: () => void;
}) {
  const name = app.id === 'other' ? (agentKey.client?.name ?? agentKey.name) : app.label;
  return (
    <>
      <div className="agents-connect-done" role="status">
        <span className="agents-done-mark" aria-hidden="true">
          <Check size={26} strokeWidth={2.5} />
        </span>
        <h3>
          <bdi>{name}</bdi> is connected
        </h3>
        <p>
          {app.id === 'chatgpt' ? (
            'Open a new ChatGPT chat to use Dispatch.'
          ) : (
            <>
              Start a new <bdi>{name}</bdi> session to use Dispatch.
            </>
          )}
        </p>
        <span className="agents-tags">
          <span className="agents-tag">{reachText(agentKey, dsps).count}</span>
          <span className="agents-tag">{toolLabels[agentKey.tools]}</span>
        </span>
      </div>
      <div className="agents-connect-footer">
        <button className="primary" onClick={close}>
          Done
        </button>
      </div>
    </>
  );
}
