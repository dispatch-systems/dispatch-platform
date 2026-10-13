// Links to Google Drive, Docs, Sheets and Slides, as someone copies them from Google: the file
// or folder each names, so pasting one on Documents opens Google's picker on it.

const ID = /^[\w-]+$/;

/** The file or folder a Google link names: none for any other text, or a published copy. */
export function driveLink(text: string): { file: string } | { folder: string } | undefined {
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return undefined;
  }
  if (url.protocol !== 'https:' || !['drive.google.com', 'docs.google.com'].includes(url.hostname))
    return undefined;
  const steps = url.pathname.split('/');
  const after = (step: string) => {
    const at = steps.indexOf(step);
    return at < 0 ? undefined : steps[at + 1];
  };
  const folder = after('folders');
  if (folder && ID.test(folder)) return { folder };
  // A published copy's address (`/d/e/…`) names no file.
  const file = after('d') ?? url.searchParams.get('id');
  return file && file !== 'e' && ID.test(file) ? { file } : undefined;
}

/** The files the Google links in `text` name, or else the first folder; none without one. */
export function driveLinks(text: string): { files: string[]; folder?: string } | undefined {
  const links = text.split(/\s+/).map(driveLink);
  const files = [...new Set(links.flatMap((link) => (link && 'file' in link ? [link.file] : [])))];
  if (files.length) return { files };
  const folder = links.find((link) => link && 'folder' in link) as { folder: string } | undefined;
  return folder && { files, folder: folder.folder };
}
