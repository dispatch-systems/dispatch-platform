/** A paper photo with optional quarter-turn rotation, encoded once before upload. */
export async function shrunk(photo: File, signal?: AbortSignal, turns = 0): Promise<Blob> {
  signal?.throwIfAborted();
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(photo);
  } catch {
    throw new Error('Choose a photo the browser can open.');
  }
  try {
    signal?.throwIfAborted();
    const scale = Math.min(1, 2000 / Math.max(bitmap.width, bitmap.height));
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
    signal?.throwIfAborted();
    if (!blob) throw new Error('Couldn’t prepare the photo. Try again.');
    return blob;
  } finally {
    bitmap.close();
  }
}
