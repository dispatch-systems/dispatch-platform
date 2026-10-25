import { useEffect, useRef, useState } from 'react';
import { countdown } from '../../../../core/shell/frontend/lib/format.js';
import { ErrorBox, Loading, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { newPairing, usePhones, type PhonePairing } from '../../api/client.js';

/** How often the dialog reads the DSP's phones, to close once the code paired one. */
const POLL = 3_000;

// Add phone: the code that pairs the next phone, as a picture its app reads and as its letters,
// with how long it lasts. A code that expires gives way to a new one at a press. It closes
// itself once a new phone appears.

export function PairDialog({ onClose }: { onClose: () => void }) {
  const [pairing, setPairing] = useState<PhonePairing>();
  const [error, setError] = useState('');
  const [now, setNow] = useState(Date.now());
  const phones = usePhones(POLL);
  const known = useRef<number>();
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
  // Closes once the DSP has a phone more than it had as the dialog opened.
  const count = phones.data?.phones.length;
  useEffect(() => {
    if (count === undefined) return;
    if (known.current === undefined) known.current = count;
    else if (count > known.current) onClose();
  }, [count, onClose]);
  const expired = pairing !== undefined && now >= pairing.expiresAt;
  return (
    <Modal title="Add phone" onClose={onClose}>
      <div className="phones-pairing">
        {error ? (
          <ErrorBox message={error} />
        ) : !pairing ? (
          <Loading />
        ) : (
          <>
            <img
              className={expired ? 'phones-code expired' : 'phones-code'}
              src={pairing.picture}
              alt={`Pairing code ${pairing.code}`}
            />
            <code className="phones-code-letters">{pairing.code}</code>
            {expired ? (
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
    </Modal>
  );
}
