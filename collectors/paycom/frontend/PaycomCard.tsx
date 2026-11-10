import { ConnectionCard } from '../../../core/collection/frontend/index.js';
import type { ConnectionCardContext } from '../../../core/shell/frontend/runtime/slots.js';
import { checks } from './checks.js';
import { PaycomConnect } from './PaycomConnect.js';

/**
 * Paycom connects through its API, with an SID and a Token, or by signing in, with the DSP's
 * client code, a username and password, and five security PINs. The API is offered only where
 * it's being tried, until its hours are proven against Paycom's own.
 */
export function PaycomCard({
  read,
  environment,
  ...context
}: ConnectionCardContext & { read: string }) {
  const api = environment !== 'production';
  return (
    <ConnectionCard
      {...context}
      provider="paycom"
      read={read}
      verification="Paycom needs your verification"
      checks={checks}
      tag={(connection) =>
        api && connection.enabled ? (connection.checks.length > 0 ? 'API' : 'Sign-in') : undefined
      }
      dialog={(dialog) => <PaycomConnect {...dialog} api={api} />}
    />
  );
}
