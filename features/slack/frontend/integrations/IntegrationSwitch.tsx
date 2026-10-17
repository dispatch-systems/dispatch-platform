import type { ListedIntegration } from '../../../../core/shell/frontend/runtime/slots.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { slackAccountChanged, switchSlackIntegration } from '../../api/client.js';
import { IntegrationNotes } from './IntegrationNotes.js';

// Switching an integration on or off in a workspace, once what that means is said, in the
// integration's own words.

export function IntegrationSwitch({
  team,
  integration,
  on,
  close,
}: {
  team: string;
  integration: ListedIntegration;
  /** Whether it's being switched on, or off. */
  on: boolean;
  /** Once it's switched, or not after all. */
  close: () => void;
}) {
  const switching = useAction(
    async () => {
      await switchSlackIntegration(team, integration.id, on);
      slackAccountChanged();
      close();
    },
    { inline: true },
  );
  const turn = on ? 'Turn on' : 'Turn off';
  return (
    <Modal title={`${turn} ${integration.label}?`} onClose={close}>
      <IntegrationNotes notes={on ? integration.turningOn : integration.turningOff} />
      <ErrorBox message={switching.error} />
      <div className="form-actions">
        <button onClick={close}>Cancel</button>
        <button className="primary" disabled={switching.busy} onClick={() => void switching.run()}>
          {turn}
        </button>
      </div>
    </Modal>
  );
}
