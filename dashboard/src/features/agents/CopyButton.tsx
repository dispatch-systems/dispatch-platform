import { useState } from 'react';
import { Check, Copy } from 'lucide-react';

/** Copies `text`, and says so for a moment. */
export function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState<'idle' | 'copied' | 'failed'>('idle');
  return (
    <button
      type="button"
      aria-label={label}
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text);
          setCopied('copied');
        } catch {
          setCopied('failed');
        }
        setTimeout(() => setCopied('idle'), 2000);
      }}
    >
      {copied === 'copied' ? <Check size={14} /> : <Copy size={14} />}
      {copied === 'copied' ? 'Copied' : copied === 'failed' ? 'Copy failed' : 'Copy'}
    </button>
  );
}
