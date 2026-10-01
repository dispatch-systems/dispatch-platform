import { useState } from 'react';
import { Download, ExternalLink } from 'lucide-react';
import { CopyButton } from './CopyButton.js';
import { KeyTest } from './KeyTest.js';
import { Setup } from './Setup.js';

/** How an agent or script reaches Dispatch with a key, and a way to test one. */
export function ConnectTab() {
  const [token, setToken] = useState('');
  const origin = window.location.origin;
  const addresses = [
    ['MCP', `${origin}/api/v1/mcp`, 'Copy MCP address'],
    ['REST API', `${origin}/api/v1`, 'Copy API address'],
  ] as const;
  return (
    <section className="agents-connect" aria-labelledby="agents-connect-title">
      <h2 id="agents-connect-title">Agents and scripts</h2>
      <Setup token="dsk_…" />
      {addresses.map(([name, address, label]) => (
        <div className="agents-endpoint" key={name}>
          <span>{name}</span>
          <div className="agents-key">
            <code>{address}</code>
            <CopyButton text={address} label={label} />
          </div>
        </div>
      ))}
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
      <div className="agents-files">
        <a href="/api/platform/agents/openapi.json" target="_blank" rel="noopener">
          <ExternalLink size={16} aria-hidden="true" />
          OpenAPI spec
        </a>
        <a href="/api/platform/agents/skill" download="SKILL.md">
          <Download size={16} aria-hidden="true" />
          Download Dispatch skill
        </a>
      </div>
    </section>
  );
}
