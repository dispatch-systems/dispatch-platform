import { useState } from 'react';
import { Download, Ellipsis, Plus, Smartphone } from 'lucide-react';
import type { DspView } from '../../../../../core/accounts/api/index.js';
import { phoneText } from '../../../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../../../core/shell/frontend/runtime/useAction.js';
import {
  Badge,
  ConfirmDialog,
  DataState,
  Popover,
} from '../../../../../core/shell/frontend/ui/index.js';
import { removePhone, usePhones } from '../../../api/client.js';
import type { PhoneHealth, PhoneRow } from '../../../api/index.js';
import { PairDialog } from './PairDialog.js';
import { RenameDialog } from './RenameDialog.js';
import './phones.css';

/** How often the card reads the DSP's phones again, as they check in. */
const POLL = 30_000;
/** How many texts waiting to go on a phone need attention: fewer go within a moment. */
const BACKLOG = 10;

// Communications' card for the DSP's dispatch phones on Settings' DSP Connections: each phone, by
// its number, with its name, model and the app's version, tagged Update while the DSP's address
// hands out a newer one, whether it's online, what needs attention on it as it last said it
// stands, and renaming or removing it; Add phone, which shows
// the code that pairs the next one, beside the app to install first; and the app's download, while
// the DSP's address hands one out.

export function PhonesCard(_: { view: DspView }) {
  const phones = usePhones(POLL);
  const app = phones.data?.app ?? undefined;
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
    <article
      className="archived-connection-card phones-connection-card"
      aria-labelledby="phones-connection"
    >
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
                      <small>
                        {[phone.name, phone.model, phone.appVersion].filter(Boolean).join(' · ')}
                      </small>
                      {phone.health && <Attention health={phone.health} />}
                    </div>
                    {phone.update && <Badge value="queued">Update</Badge>}
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
        {app && (
          <a className="phones-download" href={app.link} download>
            <Download size={16} />
            Download app
          </a>
        )}
      </footer>
      {pairing && (
        <PairDialog
          app={app}
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

/** What needs attention on a phone, as it last said it stands: nothing while all's well. */
function Attention({ health }: { health: PhoneHealth }) {
  const undownloaded = health.undownloaded ?? 0;
  const pictures = undownloaded === 1 ? 'picture' : 'pictures';
  const needs = (
    [
      [health.cantSend, 'failed', "Can't send texts"],
      [health.cantRead, 'failed', "Can't read texts"],
      [health.noSim, 'failed', 'No SIM'],
      [health.noSignal, 'queued', 'No signal'],
      [health.setupUndone, 'queued', 'Setup not finished'],
      [health.waiting >= BACKLOG, 'queued', `${health.waiting} texts waiting to go`],
      [undownloaded > 0, 'queued', `${undownloaded} ${pictures} not downloaded`],
    ] satisfies [needed: boolean, tone: string, label: string][]
  ).filter(([needed]) => needed);
  if (needs.length === 0) return null;
  return (
    <span className="phones-attention">
      {needs.map(([, tone, label]) => (
        <Badge key={label} value={tone}>
          {label}
        </Badge>
      ))}
    </span>
  );
}

/** How long a phone has been offline: "Offline · 14m", "Offline · 3h", "Offline · 2d". */
function offline(lastSeenAt: number | null) {
  if (lastSeenAt === null) return 'Offline';
  const minutes = Math.max(1, Math.floor((Date.now() - lastSeenAt) / 60_000));
  const ago =
    minutes < 60
      ? `${minutes}m`
      : minutes < 24 * 60
        ? `${Math.floor(minutes / 60)}h`
        : `${Math.floor(minutes / (24 * 60))}d`;
  return `Offline · ${ago}`;
}
