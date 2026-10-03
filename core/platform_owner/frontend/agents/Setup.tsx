import { useState } from 'react';
import { Tabs } from '../../ui/index.js';
import { setups } from '../../lib/agents.js';
import { CopyButton } from './CopyButton.js';

/** How each kind of agent connects with `token`, a tab for each. */
export function Setup({ token }: { token: string }) {
  const options = setups(window.location.origin, token);
  const [shown, setShown] = useState<string>(options[0].id);
  const setup = options.find((option) => option.id === shown) ?? options[0];
  return (
    <div className="agents-setup">
      <Tabs
        label="Agent"
        value={setup.id}
        onChange={setShown}
        items={options.map((option) => [option.id, option.label] as const)}
      />
      <pre className="agents-snippet" role="tabpanel" aria-label={setup.label}>
        <CopyButton text={setup.text} label={`Copy ${setup.label} setup`} />
        {setup.text}
      </pre>
    </div>
  );
}
