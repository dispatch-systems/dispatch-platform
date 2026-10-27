import { marked } from './marked.js';
import './name.css';

/** A name, with the letters that matched what was searched marked. */
export function Name({
  name,
  matched = [],
}: {
  name: string;
  matched?: readonly (readonly [number, number])[];
}) {
  return (
    <>
      {marked(name, matched).map((run, at) =>
        run.mark ? <mark key={at}>{run.text}</mark> : run.text,
      )}
    </>
  );
}
