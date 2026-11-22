import { jpegOf, opened } from '../../../../core/shell/frontend/lib/pictures.js';

/** A paper photo with optional quarter-turn rotation, encoded once before upload. */
export async function shrunk(photo: File, signal?: AbortSignal, turns = 0): Promise<Blob> {
  signal?.throwIfAborted();
  const bitmap = await opened(photo);
  try {
    signal?.throwIfAborted();
    const blob = await jpegOf(bitmap, 2000, turns);
    signal?.throwIfAborted();
    return blob;
  } finally {
    bitmap.close();
  }
}
