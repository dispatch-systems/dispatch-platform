import { useEffect, useState } from 'react';
import { countdown } from '../../../../../core/shell/frontend/lib/format.js';
import { ErrorBox, Loading, Modal } from '../../../../../core/shell/frontend/ui/index.js';
import { newPairing, usePairing } from '../../../api/client.js';
import type { PhoneApp, PhonePairing } from '../../../api/index.js';

/** How often the dialog asks after its code until the server says it paired, was replaced or
 * expired, to close once it paired a phone. */
const POLL = 3_000;

// Add phone: the code that pairs the next phone, as a picture its app reads and as its letters,
// with how long it lasts. One that expired, or that a newer code replaced, gives way to a new one
// at a press. It closes itself once its own code paired a phone. While the DSP's address hands out
// the app, installing it comes first: its download as a picture a phone's camera reads.

export function PairDialog({ app, onClose }: { app?: PhoneApp; onClose: () => void }) {
  const [pairing, setPairing] = useState<PhonePairing>();
  const [error, setError] = useState('');
  const [now, setNow] = useState(Date.now());
  const make = () => {
    setError('');
    setPairing(undefined);
    newPairing().then(setPairing, (failure: Error) => setError(failure.message));
  };
  useEffect(make, []);
  useEffect(() => {
    const ticking = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(ticking);
  }, []);
  const lapsed = pairing !== undefined && now >= pairing.expiresAt;
  const [asking, setAsking] = useState(true);
  const { data, refresh } = usePairing(pairing?.id, asking ? POLL : 0);
  const status = data?.status;
  useEffect(() => setAsking(status === undefined || status === 'waiting'), [status]);
  // Expired here, it asks once more: a phone may have paired since it last asked.
  useEffect(() => {
    if (lapsed) refresh();
  }, [lapsed, refresh]);
  useEffect(() => {
    if (status === 'paired') onClose();
  }, [status, onClose]);
  const over = lapsed || status === 'replaced' || status === 'expired';
  const pair = (
    <div className="phones-pairing">
      {error ? (
        <ErrorBox message={error} />
      ) : !pairing ? (
        <Loading />
      ) : (
        <>
          <img
            className={over ? 'phones-code expired' : 'phones-code'}
            src={pairing.picture}
            alt={`Pairing code ${pairing.code}`}
          />
          <code className="phones-code-letters">{pairing.code}</code>
          {over ? (
            <button className="primary" onClick={make}>
              New code
            </button>
          ) : (
            <span className="muted phones-expires">
              Expires in {countdown(new Date(pairing.expiresAt).toISOString(), now)}
            </span>
          )}
        </>
      )}
    </div>
  );
  return (
    <Modal title="Add phone" onClose={onClose}>
      {app ? (
        <div className="phones-steps">
          <section className="phones-step" aria-label="Install">
            <h3>
              <b>1</b>Install
            </h3>
            <img className="phones-code" src={app.picture} alt={`Download ${app.name}`} />
            <span className="muted">
              {app.name} {app.version}
            </span>
          </section>
          <section className="phones-step" aria-label="Pair">
            <h3>
              <b>2</b>Pair
            </h3>
            {pair}
          </section>
        </div>
      ) : (
        pair
      )}
    </Modal>
  );
}
