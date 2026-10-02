import fs from 'node:fs';
import assert from 'node:assert/strict';

/** Scan provider residue without loading an executable or profile file into memory. */
export function fileContainsAny(file: string, needles: Buffer[], chunkSize = 64 * 1024) {
  assert(Number.isInteger(chunkSize) && chunkSize > 0);
  if (!needles.length) return false;
  const overlap = Math.max(...needles.map((needle) => needle.length)) - 1;
  assert(overlap >= 0);
  const bytes = Buffer.alloc(chunkSize + overlap);
  const descriptor = fs.openSync(file, 'r');
  let carried = 0;
  try {
    for (;;) {
      const read = fs.readSync(descriptor, bytes, carried, chunkSize, null);
      if (!read) return false;
      const available = carried + read;
      if (needles.some((needle) => bytes.subarray(0, available).includes(needle))) return true;
      carried = Math.min(overlap, available);
      bytes.copy(bytes, 0, available - carried, available);
    }
  } finally {
    fs.closeSync(descriptor);
  }
}
