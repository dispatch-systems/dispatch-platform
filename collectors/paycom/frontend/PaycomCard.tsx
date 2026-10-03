import { ConnectionCard } from '../../../core/collection/frontend/index.js';
import type { ConnectionCardContext } from '../../../core/shell/frontend/runtime/slots.js';

const pins = [1, 2, 3, 4, 5];
const securityAnswers = (form: FormData) =>
  pins.map((number) => String(form.get(`pin${number}`) ?? ''));

/** Paycom signs in with the DSP's client code, a username and password, and five security PINs. */
export function PaycomCard({ read, ...context }: ConnectionCardContext & { read: string }) {
  return (
    <ConnectionCard
      {...context}
      provider="paycom"
      read={read}
      verification="Paycom needs your verification"
      credentials={{
        username: { label: 'Username', type: 'text' },
        before: (connection) => (
          <label>
            Client code
            <input
              name="clientCode"
              required
              maxLength={80}
              autoComplete="off"
              defaultValue={connection?.accountLabel ?? ''}
            />
          </label>
        ),
        after: (
          <>
            {pins.map((number) => (
              <label key={number}>
                PIN {number}
                <input
                  name={`pin${number}`}
                  type="password"
                  required
                  maxLength={64}
                  autoComplete="off"
                />
              </label>
            ))}
            <p className="muted">
              Enter all five distinct security answers in the order configured for your Paycom
              account.
            </p>
          </>
        ),
        check: (form) =>
          new Set(securityAnswers(form)).size !== 5
            ? 'Enter five distinct security PINs in their original Paycom numbering.'
            : undefined,
        values: (form) => ({
          clientCode: form.get('clientCode'),
          securityAnswers: securityAnswers(form),
        }),
      }}
    />
  );
}
