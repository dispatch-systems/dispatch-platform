/** The longest side a photo of signed paper keeps: enough to read every word. */
const LONGEST = 2000;

/**
 * A photo as a JPEG no longer on its longest side than `LONGEST`, so it's quick to send and
 * small to keep. One the browser can't read goes as it is, for the server to refuse or keep.
 */
export async function shrunk(photo: File): Promise<Blob> {
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(photo);
  } catch {
    return photo;
  }
  const scale = Math.min(1, LONGEST / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(bitmap.width * scale);
  canvas.height = Math.round(bitmap.height * scale);
  canvas.getContext('2d')?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  return new Promise((resolve) =>
    canvas.toBlob((blob) => resolve(blob ?? photo), 'image/jpeg', 0.85),
  );
}
