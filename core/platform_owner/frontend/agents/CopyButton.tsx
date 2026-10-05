import { useState } from 'react';
import { Check, Copy } from 'lucide-react';

/** Copies `text`, and says so for a moment. */
export function CopyButton({
  text,
  label,
  children = 'Copy',
  onCopy,
}: {
  text: string;
  label: string;
  /** What the button says before it copies. */
  children?: string;
  /** Runs alongside the copy, which never waits for it. */
  onCopy?: () => void;
}) {
  const [copied, setCopied] = useState<'idle' | 'copied' | 'failed'>('idle');
  return (
    <button
      type="button"
      aria-label={label}
      onClick={async () => {
        try {
          const copying = navigator.clipboard.writeText(text);
          onCopy?.();
          await copying;
          setCopied('copied');
        } catch {
          setCopied('failed');
        }
        setTimeout(() => setCopied('idle'), 2000);
      }}
    >
      {copied === 'copied' ? <Check size={14} /> : <Copy size={14} />}
      {copied === 'copied' ? 'Copied' : copied === 'failed' ? 'Copy failed' : children}
    </button>
  );
}
