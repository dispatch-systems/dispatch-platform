import { useState } from 'react';
import { Download, ExternalLink } from 'lucide-react';
import { Badge } from '../../ui/index.js';
import { AllowedApps } from './AllowedApps.js';
import { CopyButton } from './CopyButton.js';
import { KeyTest } from './KeyTest.js';
import { PairingLine, usePairing } from './Pairing.js';
import { Setup } from './Setup.js';
import { SignIn } from './SignIn.js';

/** How an app signs in with Dispatch, which apps may, or an agent or script reaches it with a
 * key. Copying a way to sign in lets apps start connecting for ten minutes. */
export function ConnectTab() {
  const [token, setToken] = useState('');
  const pairing = usePairing();
  const origin = window.location.origin;
  const addresses = [
    ['MCP', `${origin}/api/v1/mcp`, 'Copy MCP address'],
    ['REST API', `${origin}/api/v1`, 'Copy API address'],
  ] as const;
  return (
    <>
      <section className="agents-connect" aria-labelledby="agents-signin-title">
        <div className="agents-connect-title">
          <h2 id="agents-signin-title">Sign in</h2>
          <Badge value="active">Recommended</Badge>
        </div>
        <SignIn onCopy={pairing.open} />
        <PairingLine pairing={pairing} />
      </section>
      <section className="agents-connect" aria-labelledby="agents-allowed-title">
        <h2 id="agents-allowed-title">Apps that may connect</h2>
        <AllowedApps />
      </section>
      <section className="agents-connect" aria-labelledby="agents-connect-title">
        <h2 id="agents-connect-title">Keys</h2>
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
    </>
  );
}
