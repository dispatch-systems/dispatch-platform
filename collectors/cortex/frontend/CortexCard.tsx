import { ConnectionCard } from '../../../core/collection/frontend/index.js';
import type { ConnectionCardContext } from '../../../core/shell/frontend/runtime/slots.js';

/** Cortex signs in with an Amazon email address and password. */
export function CortexCard({ read, ...context }: ConnectionCardContext & { read: string }) {
  return (
    <ConnectionCard
      {...context}
      provider="cortex"
      read={read}
      verification="Finish signing in to Cortex"
      credentials={{ username: { label: 'Email address', type: 'email' } }}
    />
  );
}
