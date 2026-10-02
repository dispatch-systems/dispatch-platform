import { useState } from 'react';
import { ChevronRight, Download, ExternalLink } from 'lucide-react';
import { CopyButton } from './CopyButton.js';
import { KeyTest } from './KeyTest.js';

/** What an agent or script needs to use a key, folded away beneath the keys: Dispatch's
 * addresses, a way to test a key, and the OpenAPI spec and skill. Each new key comes with its
 * own setup. */
export function UsingKeys() {
  const [open, setOpen] = useState(false);
  const [token, setToken] = useState('');
  const origin = window.location.origin;
  const addresses = [
    ['MCP', `${origin}/api/v1/mcp`, 'Copy MCP address'],
    ['REST API', `${origin}/api/v1`, 'Copy API address'],
  ] as const;
  return (
    <details
      className="agents-remote agents-using"
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>
        <ChevronRight size={14} aria-hidden="true" />
        Using keys
        {!open && <span className="muted">· addresses, key tester, OpenAPI spec and skill</span>}
      </summary>
      <div className="agents-using-body">
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
      </div>
    </details>
  );
}
