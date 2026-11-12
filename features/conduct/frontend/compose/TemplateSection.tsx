import { useEffect, useRef, useState } from 'react';
import { blank } from '../tiers.js';

/**
 * One section of the notice. Wording that leaves blanks is edited in place around them: each
 * blank is its own box, so no edit to the wording can remove it.
 */
export function TemplateSection({
  label,
  value,
  change,
}: {
  label: string;
  value: string;
  change: (value: string) => void;
}) {
  // The wording as it last arrived from outside; what's typed here is `emitted`, not a new one.
  const [template, setTemplate] = useState({ text: value, arrival: 0 });
  const emitted = useRef(value);
  useEffect(() => {
    if (value === emitted.current) return;
    emitted.current = value;
    setTemplate(({ arrival }) => ({ text: value, arrival: arrival + 1 }));
  }, [value]);
  const emit = (text: string) => {
    emitted.current = text;
    change(text);
  };
  return (
    <div className="conduct-field" data-conduct-section>
      <span className="conduct-label">{label}</span>
      {template.text.match(blank) ? (
        <Blanks key={template.arrival} label={label} template={template.text} emit={emit} />
      ) : (
        <textarea
          aria-label={label}
          rows={4}
          maxLength={4000}
          value={value}
          onChange={(e) => emit(e.target.value)}
        />
      )}
    </div>
  );
}

/** A browser ends an editable box's last line with a second line break, so Enter adds none. */
const oneLine = (e: React.KeyboardEvent) => {
  if (e.key === 'Enter') e.preventDefault();
};

/** The browser owns the text typed into each box, so the caret stays where it is typed. */
function Blanks({
  label,
  template,
  emit,
}: {
  label: string;
  template: string;
  emit: (text: string) => void;
}) {
  const names = (template.match(blank) ?? []).map((token) => token.slice(1, -1));
  const [wording] = useState(() => template.split(blank));
  const pieces = useRef([...wording]);
  const [values, setValues] = useState<string[]>([]);
  const send = (filled: string[]) =>
    emit(
      pieces.current
        .map((piece, at) => piece + (names[at] ? filled[at] || `[${names[at]}]` : ''))
        .join(''),
    );
  return (
    <div
      className="conduct-template"
      onMouseDown={(e) => {
        if (e.target !== e.currentTarget) return;
        e.preventDefault();
        e.currentTarget.querySelector<HTMLElement>('[data-conduct-blank]')?.focus();
      }}
    >
      {wording.map((piece, at) => (
        <span key={at}>
          <span
            role="textbox"
            aria-label={label}
            tabIndex={-1}
            contentEditable="plaintext-only"
            suppressContentEditableWarning
            data-conduct-wording
            onKeyDown={oneLine}
            onInput={(e) => {
              pieces.current[at] = e.currentTarget.textContent ?? '';
              send(values);
            }}
          >
            {piece}
          </span>
          {names[at] && (
            <span
              role="textbox"
              aria-label={`${label}: ${names[at]}`}
              contentEditable="plaintext-only"
              data-conduct-blank
              data-empty={!values[at] ? '' : undefined}
              data-name={names[at]}
              onKeyDown={oneLine}
              onInput={(e) => {
                const box = e.currentTarget;
                // A box left with spaces alone is empty again, and shows its name.
                if (!box.textContent?.trim()) box.textContent = '';
                const filled = [...values];
                filled[at] = box.textContent ?? '';
                setValues(filled);
                send(filled);
              }}
            />
          )}
        </span>
      ))}
    </div>
  );
}
