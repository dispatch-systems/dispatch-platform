import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { X } from 'lucide-react';
import { messageOf } from '../lib/errors.js';
import { ErrorBox } from '../ui/ErrorBox.js';

/** A button a toast offers beside its notice, such as Undo: pressing it closes the toast. */
export type ToastAction = { label: string; run: () => void };
type Feedback = {
  error: string;
  notice: string;
  action?: ToastAction;
  /** Runs `work`, then toasts `success` or shows why it failed at the top of the page. */
  perform: (work: () => Promise<unknown>, success?: string) => Promise<boolean>;
  notify: (notice: string, action?: ToastAction) => void;
  fail: (error: string) => void;
};
const Context = createContext<Feedback | null>(null);

export function FeedbackProvider({ children }: { children: ReactNode }) {
  const [toast, setToast] = useState<{ notice: string; action?: ToastAction }>({ notice: '' }),
    [error, fail] = useState('');
  const notify = useCallback<Feedback['notify']>(
    (notice, action) => setToast({ notice, action }),
    [],
  );
  const { notice, action } = toast;
  // One with an action stays a little longer, to be pressed.
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => notify(''), action ? 8000 : 5000);
    return () => clearTimeout(timer);
  }, [toast, notice, action, notify]);
  const perform = useCallback<Feedback['perform']>(
    async (work, success) => {
      fail('');
      try {
        await work();
        if (success) notify(success);
        return true;
      } catch (error) {
        fail(messageOf(error));
        return false;
      }
    },
    [notify],
  );
  const value = useMemo(
    () => ({ error, notice, action, perform, notify, fail }),
    [error, notice, action, perform, notify],
  );
  return <Context value={value}>{children}</Context>;
}

export function useFeedback() {
  const feedback = useContext(Context);
  if (!feedback) throw new Error('useFeedback needs a FeedbackProvider');
  return feedback;
}

/** Where the page shows the app-level error and toast. */
export function FeedbackMessages() {
  const { error, notice, action, notify } = useFeedback();
  return (
    <>
      <ErrorBox message={error} />
      {notice && (
        <div className="toast" role="status">
          {notice}
          {action && (
            <button
              className="toast-action"
              onClick={() => {
                notify('');
                action.run();
              }}
            >
              {action.label}
            </button>
          )}
          <button aria-label="Dismiss notification" onClick={() => notify('')}>
            <X size={16} />
          </button>
        </div>
      )}
    </>
  );
}
