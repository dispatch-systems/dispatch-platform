import { useEffect, useState } from 'react';
import { ArrowRight, ChevronDown } from 'lucide-react';
import { dspAddress } from '../../../shell/frontend/runtime/site.js';
import { ErrorBox } from '../../../shell/frontend/ui/index.js';

export type DspSetup = {
  name: string;
  abbreviation: string;
  stationCode: string;
  timezone: string;
};
const timezones = [
  ...new Set([
    'America/Los_Angeles',
    'America/Chicago',
    'America/New_York',
    'America/Denver',
    'America/Phoenix',
    'America/Anchorage',
    'Pacific/Honolulu',
    'UTC',
    ...Intl.supportedValuesOf('timeZone'),
  ]),
].sort();

const shortCode = /^[A-Za-z0-9]{2,16}$/;

/**
 * The DSP's details. Its short code becomes its address, so the form names that address as it
 * is typed and, given `checkCode`, says whether another DSP has it; once set, it is `locked`.
 */
export function DspSetupForm({
  onSubmit,
  onBack,
  checkCode,
  locked = false,
  initial,
  busy = false,
  error = '',
  resume = false,
  hidden = false,
}: {
  onSubmit: (profile: DspSetup) => void;
  onBack?: () => void;
  checkCode?: (code: string) => Promise<{ available: boolean }>;
  locked?: boolean;
  initial?: Partial<DspSetup>;
  busy?: boolean;
  error?: string;
  resume?: boolean;
  hidden?: boolean;
}) {
  const [code, setCode] = useState(initial?.abbreviation ?? '');
  const [available, setAvailable] = useState<boolean>();
  useEffect(() => {
    setAvailable(undefined);
    if (!checkCode || locked || !shortCode.test(code)) return;
    let current = true;
    const timer = setTimeout(() => {
      checkCode(code).then(
        (answer) => current && setAvailable(answer.available),
        () => undefined,
      );
    }, 300);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [checkCode, locked, code]);
  const taken = available === false ? 'That short code is taken. Choose another.' : '';
  return (
    <form
      hidden={hidden}
      aria-busy={busy}
      onSubmit={(event) => {
        event.preventDefault();
        if (taken) return;
        const form = new FormData(event.currentTarget);
        onSubmit({
          name: String(form.get('name')).trim(),
          abbreviation: String(form.get('abbreviation')).trim(),
          stationCode: String(form.get('stationCode')).toUpperCase(),
          timezone: String(form.get('timezone')),
        });
      }}
    >
      <div className="onboarding-fields">
        <label className="onboarding-wide">
          <span>
            DSP name{' '}
            <span className="onboarding-example" aria-hidden="true">
              (e.g. Northstar Logistics)
            </span>
          </span>
          <input
            name="name"
            aria-label="DSP name"
            placeholder="Your DSP name"
            autoComplete="organization"
            required
            minLength={2}
            maxLength={100}
            pattern=".*\S.*"
            defaultValue={initial?.name}
            disabled={busy}
          />
        </label>
        <label>
          <span>
            Short code{' '}
            <span className="onboarding-example" aria-hidden="true">
              (e.g. NSTL)
            </span>
          </span>
          <input
            name="abbreviation"
            aria-label="Short code"
            aria-describedby="onboarding-address"
            placeholder="Your DSP short code"
            required
            minLength={2}
            maxLength={16}
            pattern="[A-Za-z0-9]{2,16}"
            title="2 to 16 letters and numbers"
            defaultValue={initial?.abbreviation}
            readOnly={locked}
            disabled={busy}
            onChange={(event) => setCode(event.currentTarget.value.trim())}
          />
        </label>
        <label>
          <span>
            Station code{' '}
            <span className="onboarding-example" aria-hidden="true">
              (e.g. TST1)
            </span>
          </span>
          <input
            name="stationCode"
            aria-label="Station code"
            placeholder="Your station code"
            required
            pattern="[A-Za-z0-9]{3,8}"
            maxLength={8}
            defaultValue={initial?.stationCode}
            disabled={busy}
          />
        </label>
        <p
          id="onboarding-address"
          className="onboarding-wide onboarding-address"
          aria-live="polite"
        >
          {shortCode.test(code) ? (
            <>
              Your dashboard will be at <strong>{new URL(dspAddress(code)).host}</strong>
              {available === true && ' · Available'}
            </>
          ) : (
            'Letters and numbers. It becomes your dashboard’s address.'
          )}
        </p>
        <label className="onboarding-wide">
          <span>Business timezone</span>
          <span className="onboarding-select">
            <select
              name="timezone"
              required
              defaultValue={initial?.timezone || 'America/Los_Angeles'}
              disabled={busy}
            >
              {timezones.map((timezone) => (
                <option key={timezone}>{timezone}</option>
              ))}
            </select>
            <ChevronDown size={18} aria-hidden="true" />
          </span>
        </label>
      </div>
      <ErrorBox message={taken || error} />
      <div className="onboarding-actions">
        <button className="primary" disabled={busy}>
          {busy ? 'Saving…' : resume ? 'Save DSP details' : 'Continue to profile'}
          <ArrowRight size={21} aria-hidden="true" />
        </button>
        {onBack && (
          <button type="button" className="onboarding-back" disabled={busy} onClick={onBack}>
            Back to sign in
          </button>
        )}
      </div>
    </form>
  );
}
