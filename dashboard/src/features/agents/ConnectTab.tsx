import { useState } from 'react';
import { setupSnippet } from '../../lib/agents.js';
import { CopyButton } from './CopyButton.js';
import { KeyTest } from './KeyTest.js';

/** How an agent or script reaches Dispatch with a key, and a way to test one. */
export function ConnectTab() {
  const [token, setToken] = useState('');
  const origin = window.location.origin;
  const snippet = setupSnippet(origin, 'dsk_…');
  return (
    <section className="agents-connect" aria-labelledby="agents-connect-title">
      <h2 id="agents-connect-title">Agents and scripts</h2>
      <pre className="agents-snippet">
        <CopyButton text={snippet} label="Copy setup" />
        {snippet}
      </pre>
      <div className="agents-endpoint">
        <span>REST API</span>
        <div className="agents-key">
          <code>{origin}/api/v1</code>
          <CopyButton text={`${origin}/api/v1`} label="Copy API address" />
        </div>
      </div>
      <div className="agents-test">
        <input
          type="password"
          aria-label="Key to test"
          placeholder="Paste a key to test it"
          autoComplete="off"
          value={token}
          onChange={(event) => setToken(event.target.value)}
        />
        <KeyTest token={token} label="Test key" />
      </div>
    </section>
  );
}
