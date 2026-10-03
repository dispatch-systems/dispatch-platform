import type { AgentKey } from '../../../../shared/contracts/index.js';
import { ConfirmDialog } from '../../ui/index.js';
import { useAction } from '../../app/useAction.js';
import { revokeAgentKey } from '../../app/endpoints.js';

/** Asks before revoking a key or a connected app, then revokes it. */
export function RevokeDialog({
  agentKey,
  noun,
  close,
  revoked,
}: {
  agentKey: Pick<AgentKey, 'id' | 'name'>;
  noun: 'key' | 'app';
  close: () => void;
  revoked: () => void;
}) {
  const revoke = useAction(
    async () => {
      await revokeAgentKey(agentKey.id);
      revoked();
    },
    { success: noun === 'key' ? 'Key revoked' : 'App revoked' },
  );
  return (
    <ConfirmDialog
      title={
        <>
          Revoke <bdi>{agentKey.name}</bdi>?
        </>
      }
      confirm={`Revoke ${noun}`}
      tone="danger"
      busy={revoke.busy}
      onCancel={close}
      onConfirm={async () => {
        if (await revoke.run()) close();
      }}
    >
      Anything using it loses access at once. This can’t be undone.
    </ConfirmDialog>
  );
}
