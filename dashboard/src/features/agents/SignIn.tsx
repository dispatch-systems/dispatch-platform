import { Fragment, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { Tabs } from '../../ui/index.js';
import { signIns } from '../../lib/agents.js';
import { CopyButton } from './CopyButton.js';

/** One line to copy: a command or an address. It wraps between words, so a flag never breaks
 * at its hyphen; only an address may break anywhere. */
function Line({ text, label, onCopy }: { text: string; label: string; onCopy: () => void }) {
  return (
    <div className="agents-key agents-line">
      <code>
        {text.split(' ').map((word, index) => (
          <Fragment key={index}>
            {index > 0 && ' '}
            {word.includes('://') ? word : <span>{word}</span>}
          </Fragment>
        ))}
      </code>
      <CopyButton text={text} label={label} onCopy={onCopy} />
    </div>
  );
}

/** How each app signs in with Dispatch, a tab for each. Copying any of it calls `onCopy`. */
export function SignIn({ onCopy }: { onCopy: () => void }) {
  const { mcp, terminals, json, bridge } = signIns(window.location.origin);
  const tabs = [
    ...terminals.map((app) => [app.id, app.label] as const),
    ['chatgpt', 'ChatGPT'] as const,
    ['other', 'Other apps'] as const,
  ];
  const [shown, setShown] = useState<string>(tabs[0]![0]);
  const app = terminals.find((terminal) => terminal.id === shown);
  return (
    <div className="agents-setup">
      <Tabs label="App" value={shown} onChange={setShown} items={tabs} />
      <div
        key={shown}
        className="agents-signin"
        role="tabpanel"
        aria-label={tabs.find(([id]) => id === shown)![1]}
      >
        {app ? (
          <>
            <Line text={app.command} label={`Copy ${app.label} command`} onCopy={onCopy} />
            <p>{app.next}</p>
            <p className="agents-prompt">
              Or let {app.label} set it up:
              <CopyButton text={app.prompt} label={`Copy prompt for ${app.label}`} onCopy={onCopy}>
                Copy prompt
              </CopyButton>
            </p>
            <details className="agents-remote">
              <summary>
                <ChevronRight size={14} aria-hidden="true" />
                On another machine?
              </summary>
              <p>{app.remote.lead}</p>
              {app.remote.command && (
                <Line
                  text={app.remote.command}
                  label={`Copy ${app.label} remote command`}
                  onCopy={onCopy}
                />
              )}
              <p>
                Open the link in any browser and approve. Then copy the address the browser ends on,
                even if the page doesn’t load, and paste it into the terminal.
              </p>
            </details>
          </>
        ) : shown === 'chatgpt' ? (
          <>
            <ol className="agents-steps">
              <li>
                On chatgpt.com, open <strong>Plugins</strong>, click <strong>Add</strong>, then{' '}
                <strong>Create custom MCP server</strong> (or <strong>Create MCP App</strong>).
              </li>
              <li>
                Name: <strong>Dispatch</strong>
              </li>
              <li>
                URL:
                <Line text={mcp} label="Copy MCP address" onCopy={onCopy} />
              </li>
              <li>
                Authentication: <strong>OAuth</strong>
              </li>
              <li>
                Tick the confirmation and click <strong>Create</strong>.
              </li>
              <li>A Dispatch page opens — approve it there.</li>
            </ol>
            <p className="muted">
              Needs ChatGPT Plus, Pro, Business or Enterprise. Then Dispatch works in ChatGPT on the
              web, desktop and mobile.
            </p>
          </>
        ) : (
          <>
            <Line text={mcp} label="Copy MCP address" onCopy={onCopy} />
            <p>Add it as an HTTP MCP server — the app opens Dispatch to sign in.</p>
            <p>Or in a JSON config:</p>
            <pre className="agents-snippet">
              <CopyButton text={json} label="Copy JSON config" onCopy={onCopy} />
              {json}
            </pre>
            <p>For apps that only run local servers:</p>
            <Line text={bridge} label="Copy local server command" onCopy={onCopy} />
            <p>For scripts, the OpenAI API or an app that can’t sign in, use a key below.</p>
          </>
        )}
      </div>
    </div>
  );
}
