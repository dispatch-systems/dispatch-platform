import { useState } from 'react';
import { Ellipsis, Plus, Smartphone } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Badge, ConfirmDialog, DataState, Popover } from '../../../../core/shell/frontend/ui/index.js';
import { removePhone, usePhones, type PhoneRow } from '../../api/client.js';
import { PairDialog } from './PairDialog.js';
import { RenameDialog } from './RenameDialog.js';
import './phones.css';

/** How often the card reads the DSP's phones again, as they check in. */
const POLL = 30_000;

// Phones' card on Settings' DSP Connections: each of the DSP's phones, by its number, with its
// name, model and the app's version, whether it's online, and renaming or removing it; and
// Add phone, which shows the code that pairs the next one.

export function PhonesCard(_: { view: DspView }) {
  const phones = usePhones(POLL);
  const [pairing, setPairing] = useState(false);
  const [renaming, setRenaming] = useState<PhoneRow>();
  const [removing, setRemoving] = useState<PhoneRow>();
  const remove = useAction(
    async (id: string) => {
      await removePhone(id);
      phones.refresh();
      setRemoving(undefined);
    },
    { success: 'Phone removed' },
  );
  return (
    <article className="archived-connection-card phones-connection-card" aria-labelledby="phones-connection">
      <header>
        <h3 id="phones-connection">
          <Smartphone size={20} className="phones-mark" />
          Phones
        </h3>
      </header>
      <div className="archived-connection-content">
        <DataState data={phones.data} error={phones.error} retry={phones.refresh}>
          {(data) =>
            data.phones.length === 0 ? (
              <div role="status">
                <Badge value="idle">No phone</Badge>
              </div>
            ) : (
              <ul className="phones-list" aria-label="Phones">
                {data.phones.map((phone) => (
                  <li key={phone.id} className="phones-row">
                    <span className="phones-row-mark" aria-hidden>
                      <Smartphone size={18} />
                    </span>
                    <div className="phones-row-body">
                      <strong>{phone.number ? phoneText(phone.number) : phone.name}</strong>
                      <small>{[phone.name, phone.model, phone.appVersion].filter(Boolean).join(' · ')}</small>
                    </div>
                    {phone.online ? (
                      <Badge value="ready">Online</Badge>
                    ) : (
                      <Badge value="failed">{offline(phone.lastSeenAt)}</Badge>
                    )}
                    <Popover
                      className="row-menu"
                      label={`Actions for ${phone.name}`}
                      trigger={<Ellipsis size={18} />}
                      anchored
                    >
                      <button onClick={() => setRenaming(phone)}>Rename</button>
                      <button className="danger" onClick={() => setRemoving(phone)}>
                        Remove
                      </button>
                    </Popover>
                  </li>
                ))}
              </ul>
            )
          }
        </DataState>
      </div>
      <footer>
        <button className="primary" onClick={() => setPairing(true)}>
          <Plus size={16} />
          Add phone
        </button>
      </footer>
      {pairing && (
        <PairDialog
          onClose={() => {
            setPairing(false);
            phones.refresh();
          }}
        />
      )}
      {renaming && (
        <RenameDialog
          phone={renaming}
          onClose={(renamed) => {
            setRenaming(undefined);
            if (renamed) phones.refresh();
          }}
        />
      )}
      {removing && (
        <ConfirmDialog
          title={`Remove ${removing.number ? phoneText(removing.number) : removing.name}?`}
          confirm="Remove"
          tone="danger"
          busy={remove.busy}
          onConfirm={() => void remove.run(removing.id)}
          onCancel={() => setRemoving(undefined)}
        >
          {null}
        </ConfirmDialog>
      )}
    </article>
  );
}

/** How long a phone has been offline: "Offline · 14m", "Offline · 3h", "Offline · 2d". */
function offline(lastSeenAt: number | null) {
  if (lastSeenAt === null) return 'Offline';
  const minutes = Math.max(1, Math.floor((Date.now() - lastSeenAt) / 60_000));
  const ago =
    minutes < 60 ? `${minutes}m` : minutes < 24 * 60 ? `${Math.floor(minutes / 60)}h` : `${Math.floor(minutes / (24 * 60))}d`;
  return `Offline · ${ago}`;
}
