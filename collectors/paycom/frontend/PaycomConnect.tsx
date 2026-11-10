import { useState, type FormEvent, type ReactNode } from 'react';
import { Copy } from 'lucide-react';
import type { Connection } from '../../../core/collection/api/index.js';
import { toClipboard } from '../../../core/shell/frontend/runtime/clipboard.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { field } from '../../../core/shell/frontend/lib/form.js';
import { ErrorBox, Modal } from '../../../core/shell/frontend/ui/index.js';
import { functions } from './checks.js';
import './paycom.css';

// Connecting Paycom, a step at a time: which way, then what to do in Paycom before anything is
// entered here. Through its API, a DSP asks for access and switches four functions on; signing
// in, it makes each security question a PIN's, which is how Dispatch tells them apart.

type Method = 'api' | 'signin';
type Step = 'method' | 'access' | 'functions' | 'token' | 'questions' | 'login';

const pins = [1, 2, 3, 4, 5];
const securityAnswers = (form: FormData) => pins.map((number) => field(form, `pin${number}`));

/** The steps of each way, in order. */
const steps: Record<Method, Step[]> = {
  api: ['access', 'functions', 'token'],
  signin: ['questions', 'login'],
};

const failed = (connection: Connection, id: string) =>
  connection.checks.some((check) => check.id === id && check.state === 'failed');

/** Where the dialog opens: on the step that puts right what the connection's checks found. */
function first(connection: Connection, api: boolean, fixing: boolean): Step {
  if (!api) return 'questions';
  if (!fixing) return 'method';
  if (failed(connection, 'credentials')) return 'access';
  if (functions.some((each) => failed(connection, each.id))) return 'functions';
  return 'token';
}

export function PaycomConnect({
  connection,
  development,
  api,
  fixing,
  connect,
  check,
  close,
}: {
  connection: Connection;
  development: boolean;
  /** Whether connecting through Paycom's API is offered here. */
  api: boolean;
  fixing: boolean;
  connect: (credentials: Record<string, unknown>, direct?: boolean) => void;
  check: () => void;
  close: () => void;
}) {
  const [step, setStep] = useState(() => first(connection, api, fixing));
  // A DSP not yet connected starts on the API; one that is, on the way it's connected.
  const [method, setMethod] = useState<Method>(
    api && (!connection.enabled || connection.checks.length > 0) ? 'api' : 'signin',
  );
  const [refused, setRefused] = useState('');
  const copy = useAction((address: string) => toClipboard(address), { success: 'Address copied' });
  const order = steps[method];
  const at = order.indexOf(step);
  const back = at > 0 ? order[at - 1] : at === 0 && api ? 'method' : undefined;
  const next = order[at + 1];
  const off = functions.filter((each) => failed(connection, each.id));

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    if (step === 'token') {
      const periodStart = field(form, 'periodStart');
      if (periodStart && new Date(`${periodStart}T00:00:00Z`).getUTCDay() !== 0) {
        setRefused('Pay periods start on a Sunday. Pick the Sunday one began.');
        return;
      }
      connect(
        {
          sid: field(form, 'sid').trim(),
          token: field(form, 'token').trim(),
          ...(periodStart ? { periodStart } : {}),
        },
        true,
      );
      return;
    }
    if (new Set(securityAnswers(form)).size !== 5) {
      setRefused('Enter five distinct security PINs in their original Paycom numbering.');
      return;
    }
    connect({
      clientCode: form.get('clientCode'),
      username: form.get('username'),
      password: form.get('password'),
      securityAnswers: securityAnswers(form),
    });
  }

  /** A step's buttons, under what it shows: where it is among its way's, back, and on. */
  const actions = (primary: ReactNode) => (
    <div className="form-actions">
      {at >= 0 && (
        <span className="muted">
          Step {at + 1} of {order.length}
        </span>
      )}
      {back ? (
        <button type="button" onClick={() => setStep(back)}>
          Back
        </button>
      ) : (
        <button type="button" onClick={close}>
          Cancel
        </button>
      )}
      {primary}
    </div>
  );
  const onward = next && (
    <button type="button" className="primary" onClick={() => setStep(next)}>
      Next
    </button>
  );

  if (step === 'method')
    return (
      <Modal title="Connect Paycom" onClose={close} initialFocus="input:checked">
        <div className="paycom-methods" role="radiogroup" aria-label="How to connect">
          {(
            [
              ['api', 'Paycom API', 'SID and Token'],
              ['signin', 'Paycom sign-in', 'Username, password and PINs'],
            ] as const
          ).map(([value, name, takes]) => (
            <label key={value}>
              <input
                type="radio"
                name="method"
                checked={method === value}
                onChange={() => setMethod(value)}
              />
              <span>
                <strong>{name}</strong>
                <span className="muted">{takes}</span>
              </span>
            </label>
          ))}
        </div>
        {actions(
          <button type="button" className="primary" onClick={() => setStep(steps[method][0]!)}>
            Next
          </button>,
        )}
      </Modal>
    );

  if (step === 'access')
    return (
      <Modal title="Ask for API access" onClose={close} initialFocus="button.primary">
        <p className="notice">
          Contact whoever handles the Paycom API for your company. They will need Dispatch's address
          to give you an SID and a Token.
        </p>
        {connection.sourceAddress && (
          <div className="paycom-address">
            <div>
              <span className="muted">Dispatch's address</span>
              <code>{connection.sourceAddress}</code>
            </div>
            <button type="button" onClick={() => void copy.run(connection.sourceAddress!)}>
              <Copy size={16} />
              Copy
            </button>
          </div>
        )}
        {actions(onward)}
      </Modal>
    );

  if (step === 'functions')
    return (
      <Modal title="Turn these on in Paycom" onClose={close} initialFocus="button.primary">
        <p className="muted">User Options › User Access and Security › API Setup</p>
        <ul className="paycom-list">
          {functions.map((each) => (
            <li key={each.id} className={off.includes(each) ? 'off' : undefined}>
              {each.label}
              {off.includes(each) && <span>Not on yet</span>}
            </li>
          ))}
        </ul>
        {actions(
          off.length > 0 ? (
            <button type="button" className="primary" onClick={check}>
              Check again
            </button>
          ) : (
            onward
          ),
        )}
      </Modal>
    );

  if (step === 'questions')
    return (
      <Modal title="Check your security questions" onClose={close} initialFocus="button.primary">
        <p className="notice">
          In Paycom, each of your five security questions must be the PIN # one. Check this before
          you enter your login.
        </p>
        <ul className="paycom-list">
          {pins.map((number) => (
            <li key={number}>
              Security question {number}
              <span>PIN #{number}</span>
            </li>
          ))}
        </ul>
        {actions(onward)}
      </Modal>
    );

  return (
    <Modal
      title={step === 'token' ? 'Enter your SID and Token' : 'Enter your Paycom login'}
      onClose={close}
    >
      {development && (
        <div className="notice">
          {step === 'token'
            ? 'Development uses synthetic data. Use any SID and Token; use Token “refused-token” or “punch-history-off” to see a check fail.'
            : 'Development uses synthetic data. Use any test credentials; use password “require-verification” to exercise verification.'}
        </div>
      )}
      <form onSubmit={submit} onInput={() => setRefused('')}>
        {step === 'token' ? (
          <>
            <label>
              SID
              <input name="sid" required maxLength={200} autoComplete="off" />
            </label>
            <label>
              Token
              <input
                name="token"
                type="password"
                required
                maxLength={256}
                autoComplete="new-password"
              />
            </label>
            {failed(connection, 'pay_period') && (
              <label>
                First day of a pay period
                <input name="periodStart" type="date" required />
              </label>
            )}
          </>
        ) : (
          <>
            <label>
              Client code
              <input
                name="clientCode"
                required
                maxLength={80}
                autoComplete="off"
                defaultValue={connection.accountLabel ?? ''}
              />
            </label>
            <label>
              Username
              <input name="username" required maxLength={200} autoComplete="off" />
            </label>
            <label>
              Password
              <input
                name="password"
                type="password"
                required
                maxLength={256}
                autoComplete="new-password"
              />
            </label>
            <div className="paycom-pins">
              {pins.map((number) => (
                <label key={number}>
                  PIN #{number}
                  <input
                    name={`pin${number}`}
                    type="password"
                    required
                    maxLength={64}
                    autoComplete="off"
                  />
                </label>
              ))}
            </div>
          </>
        )}
        <ErrorBox message={refused} />
        {actions(<button className="primary">Connect</button>)}
      </form>
    </Modal>
  );
}
