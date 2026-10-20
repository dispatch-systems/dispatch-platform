/**
 * Puts `text` on the clipboard, or text still being made, as a link a click asks the server
 * for: Safari lets a page write there only while answering the click, so the clipboard is
 * given what's to come. Without the clipboard's API, as at an address that isn't secure, text
 * known already is copied as a selection is; text still being made can't be, and rejects.
 */
export async function toClipboard(text: string | Promise<string>) {
  if (!navigator.clipboard) {
    if (typeof text !== 'string' || !copySelected(text)) throw new Error("Couldn't copy.");
    return;
  }
  if (
    typeof text !== 'string' &&
    typeof ClipboardItem !== 'undefined' &&
    navigator.clipboard.write
  ) {
    const blob = text.then((value) => new Blob([value], { type: 'text/plain' }));
    await navigator.clipboard.write([new ClipboardItem({ 'text/plain': blob })]);
  } else await navigator.clipboard.writeText(await text);
}

/** Copies `text` as the browser copies a selection, while answering a click: whether it did. */
function copySelected(text: string) {
  const field = document.createElement('textarea');
  field.value = text;
  field.readOnly = true;
  field.style.position = 'fixed';
  field.style.opacity = '0';
  document.body.append(field);
  field.select();
  try {
    return document.execCommand('copy');
  } finally {
    field.remove();
  }
}
