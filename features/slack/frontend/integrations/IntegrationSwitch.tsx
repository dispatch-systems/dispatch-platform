import type { ListedIntegration } from '../../../../core/shell/frontend/runtime/slots.js';
import { Modal } from '../../../../core/shell/frontend/ui/index.js';
import { IntegrationNotes } from './IntegrationNotes.js';

// Asks before an integration is switched on or off in a workspace, saying what that means in the
// integration's own words.

export function IntegrationSwitch({
  integration,
  on,
  confirm,
  close,
}: {
  integration: ListedIntegration;
  /** Whether it's being switched on, or off. */
  on: boolean;
  /** Once it's asked for. */
  confirm: () => void;
  /** Not after all. */
  close: () => void;
}) {
  const turn = on ? 'Turn on' : 'Turn off';
  return (
    <Modal title={`${turn} ${integration.label}?`} onClose={close}>
      <IntegrationNotes notes={on ? integration.turningOn : integration.turningOff} />
      <div className="form-actions">
        <button onClick={close}>Cancel</button>
        <button className="primary" onClick={confirm}>
          {turn}
        </button>
      </div>
    </Modal>
  );
}
