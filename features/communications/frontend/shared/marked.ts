/** `name` in runs, each marked when its letters matched what was searched (`matched`). */
export function marked(name: string, matched: readonly (readonly [number, number])[]) {
  const runs: { text: string; mark: boolean }[] = [];
  let at = 0;
  for (const [start, end] of matched) {
    if (start > at) runs.push({ text: name.slice(at, start), mark: false });
    runs.push({ text: name.slice(start, end), mark: true });
    at = end;
  }
  if (at < name.length) runs.push({ text: name.slice(at), mark: false });
  return runs;
}
