import { KeyRound, LockKeyhole } from 'lucide-react';
import type { AgentKeyCreated } from '../../../../shared/contracts/index.js';
import { Modal } from '../../ui/index.js';
import { CopyButton } from './CopyButton.js';
import { KeyTest } from './KeyTest.js';
import { Setup } from './Setup.js';

/** A new key, shown this once, with how an agent uses it. */
export function KeyReady({ created, close }: { created: AgentKeyCreated; close: () => void }) {
  return (
    <Modal title={`${created.key.name} is ready`} onClose={close} dismissible={false}>
      <div className="agents-ready">
        <div className="agents-key">
          <KeyRound size={16} aria-hidden="true" />
          <code>{created.token}</code>
          <CopyButton text={created.token} label="Copy key" />
        </div>
        <div className="notice">
          <LockKeyhole size={16} aria-hidden="true" />
          <span>Copy it now. Dispatch keeps only a fingerprint and can’t show it again.</span>
        </div>
        <Setup token={created.token} />
        <div className="agents-test">
          <KeyTest token={created.token} />
        </div>
        <div className="form-actions">
          <button className="primary" onClick={close}>
            Done
          </button>
        </div>
      </div>
    </Modal>
  );
}
