import { useState } from 'react';
import { CircleCheck, Send } from 'lucide-react';
import type { AgentWhoami } from '../../api/index.js';
import { agentWhoami } from '../../api/client.js';
import { errorLabel } from '../../../shell/frontend/runtime/api.js';
import { accessLabels } from './agents.js';
import { dateFormatter } from '../../../shell/frontend/lib/date-format.js';

const failures: Record<string, string> = {
  agent_key_required: 'Paste a key to test.',
  agent_key_invalid: 'That isn’t a key for this Dispatch.',
  agent_key_expired: 'That key has expired.',
  agent_key_revoked: 'That key was revoked.',
  rate_limited: 'That key made too many calls this minute. Try again shortly.',
  network_error: 'Dispatch couldn’t be reached.',
};

/** What a successful test says: the key's access, its DSPs and today's date. */
export function whoamiText(whoami: AgentWhoami) {
  const dsps = whoami.dsps.length === 1 ? '1 DSP' : `${whoami.dsps.length} DSPs`;
  const today = dateFormatter('en-US', { weekday: 'long', month: 'short', day: 'numeric' }).format(
    new Date(whoami.now),
  );
  return `Connected · ${accessLabels[whoami.key.access]} · ${dsps} · ${today}`;
}

/** Sends one request with a key, as an agent would, and shows what came back. */
export function KeyTest({ token, label = 'Send test request' }: { token: string; label?: string }) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  return (
    <>
      <button
        type="button"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          const answer = await agentWhoami(token.trim());
          setBusy(false);
          setResult(
            answer.ok
              ? { ok: true, text: whoamiText(answer.value) }
              : {
                  ok: false,
                  text:
                    failures[answer.error] ??
                    errorLabel(answer.error) ??
                    (answer.error.startsWith('http_')
                      ? `Dispatch answered ${answer.error.slice(5)} instead of the test.`
                      : 'The test request failed.'),
                },
          );
        }}
      >
        <Send size={14} />
        {label}
      </button>
      {result && (
        <span className={`agents-result ${result.ok ? 'ok' : 'failed'}`} role="status">
          {result.ok && <CircleCheck size={16} />}
          {result.text}
        </span>
      )}
    </>
  );
}
