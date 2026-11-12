import { useEffect, useRef, useState } from 'react';
import { blank } from '../tiers.js';

/** Keep token identity while its value is typed; reparsing the assembled text loses filled tokens. */
export function TemplateSection({
  label,
  value,
  change,
}: {
  label: string;
  value: string;
  change: (value: string) => void;
}) {
  const [source, setSource] = useState(value);
  const [values, setValues] = useState<string[]>([]);
  const [editing, setEditing] = useState(!value.match(blank));
  const emitted = useRef(value);
  useEffect(() => {
    if (value === emitted.current) return;
    emitted.current = value;
    setSource(value);
    setValues([]);
    setEditing(!value.match(blank));
  }, [value]);
  const tokens = source.match(blank) ?? [];
  const pieces = source.split(blank);
  const emit = (text: string) => {
    emitted.current = text;
    change(text);
  };
  return (
    <div className="conduct-field conduct-template-section" data-conduct-section>
      <div className="conduct-section-label">
        <span className="conduct-label">{label}</span>
        {!editing && (
          <button type="button" className="text-button" onClick={() => setEditing(true)}>
            Edit text
          </button>
        )}
      </div>
      {editing ? (
        <textarea
          aria-label={label}
          rows={4}
          maxLength={4000}
          value={value}
          onChange={(e) => emit(e.target.value)}
        />
      ) : (
        <div className="conduct-template">
          {pieces.map((piece, at) => (
            <span key={at}>
              {piece}
              {tokens[at] && (
                <input
                  aria-label={`${label}: ${tokens[at].slice(1, -1)}`}
                  data-conduct-blank
                  data-empty={!values[at] ? '' : undefined}
                  placeholder={tokens[at].slice(1, -1)}
                  value={values[at] ?? ''}
                  maxLength={4000}
                  onChange={(e) => {
                    const next = [...values];
                    next[at] = e.target.value;
                    const text = pieces
                      .map((part, n) => part + (tokens[n] ? next[n] || tokens[n] : ''))
                      .join('');
                    if ([...text].length <= 4000) {
                      setValues(next);
                      emit(text);
                    }
                  }}
                />
              )}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
