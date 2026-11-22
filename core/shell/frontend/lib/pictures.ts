/** The picture `photo` holds, opened upright as its camera took it, for the caller to close:
 * refused when the browser can't open it. */
export async function opened(photo: Blob): Promise<ImageBitmap> {
  try {
    return await createImageBitmap(photo);
  } catch {
    throw new Error('Choose a photo the browser can open.');
  }
}

/** `bitmap` as a JPEG at most `longest` pixels on its longest side, turned `turns` quarter turns,
 * on white where it's clear. */
export async function jpegOf(bitmap: ImageBitmap, longest: number, turns = 0): Promise<Blob> {
  const scale = Math.min(1, longest / Math.max(bitmap.width, bitmap.height));
  const width = Math.round(bitmap.width * scale),
    height = Math.round(bitmap.height * scale);
  const canvas = document.createElement('canvas');
  canvas.width = turns % 2 ? height : width;
  canvas.height = turns % 2 ? width : height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Couldn’t prepare the photo. Try again.');
  context.fillStyle = '#fff';
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.translate(canvas.width / 2, canvas.height / 2);
  context.rotate((turns * Math.PI) / 2);
  context.drawImage(bitmap, -width / 2, -height / 2, width, height);
  const blob = await new Promise<Blob | null>((resolve) =>
    canvas.toBlob(resolve, 'image/jpeg', 0.85),
  );
  if (!blob) throw new Error('Couldn’t prepare the photo. Try again.');
  return blob;
}

/**
 * `photo` as it goes where nothing larger than `longest` pixels on its longest side is shown: a
 * JPEG that size when it's larger, or when it's more than `most` bytes and that makes it smaller.
 * A GIF, which may move, and a picture the browser can't open, are left for the server to judge.
 */
export async function within(
  photo: File,
  longest: number,
  most: number,
  signal?: AbortSignal,
): Promise<File> {
  if (photo.type === 'image/gif') return photo;
  let bitmap: ImageBitmap;
  try {
    bitmap = await opened(photo);
  } catch {
    return photo;
  }
  try {
    signal?.throwIfAborted();
    const fits = Math.max(bitmap.width, bitmap.height) <= longest;
    if (fits && photo.size <= most) return photo;
    const jpeg = await jpegOf(bitmap, longest);
    signal?.throwIfAborted();
    // One that fits keeps its own bytes unless the JPEG's are fewer; a larger one is never sent.
    if (fits && jpeg.size >= photo.size) return photo;
    const name = `${photo.name.replace(/\.[^.]*$/, '') || 'photo'}.jpg`;
    return new File([jpeg], name, { type: 'image/jpeg' });
  } finally {
    bitmap.close();
  }
}
