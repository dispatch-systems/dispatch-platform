import { beginBrowserWrite } from './browser-update.js';
import { ApiError, csrf, errorLabel, view } from './api.js';

// Files to and from the server, for a page that moves them: kept out of `api` so only the
// pages that need them load them.

/** The error a failed answer names, worded as `api` words it. */
function failed(status: number, value: { error?: string; message?: string }, request?: string) {
  const code = value.error ?? 'request_failed';
  if (code === 'dsp_view_expired') window.dispatchEvent(new Event('dispatch-view-expired'));
  if (status === 401) window.dispatchEvent(new Event('dispatch-signed-out'));
  return new ApiError(
    code,
    errorLabel(code) ?? value.message ?? 'The request could not be completed.',
    status,
    request,
  );
}

/**
 * Sends `file` as the body of `url`, a route that takes uploads, telling `progress` how many of
 * its bytes have gone. Answers what the route answers.
 */
export function upload<T>(
  url: string,
  file: Blob,
  progress?: (sent: number) => void,
  signal?: AbortSignal,
): Promise<T> {
  const finish = beginBrowserWrite();
  return new Promise<T>((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open('POST', url);
    request.setRequestHeader('Content-Type', 'application/octet-stream');
    request.setRequestHeader('X-CSRF-Token', csrf);
    if (view) request.setRequestHeader('X-Dispatch-View', view);
    request.upload.onprogress = (event) => progress?.(event.loaded);
    request.onload = () => {
      let value: unknown;
      try {
        value = JSON.parse(request.responseText);
      } catch {
        value = {};
      }
      if (request.status >= 200 && request.status < 300) resolve(value as T);
      else
        reject(
          failed(
            request.status,
            value as { error?: string },
            request.getResponseHeader('x-request-id') ?? undefined,
          ),
        );
    };
    request.onerror = () =>
      reject(
        new ApiError('upload_failed', 'The upload stopped. Check the connection and try again.', 0),
      );
    request.onabort = () => reject(new DOMException('The upload was stopped.', 'AbortError'));
    signal?.addEventListener('abort', () => request.abort(), { once: true });
    request.send(file);
  }).finally(finish);
}

/** The name `Content-Disposition` gives a file, exactly when it says so. */
export function savedName(disposition: string | null): string | undefined {
  const exact = /filename\*=UTF-8''([^;]+)/i.exec(disposition ?? '')?.[1];
  if (exact)
    try {
      return decodeURIComponent(exact);
    } catch {
      // A malformed name falls back to the plain one.
    }
  return /filename="([^"]*)"/i.exec(disposition ?? '')?.[1];
}

/** What `url` answers, asked as the page's own calls are: with the session and the DSP's view. */
async function fetched(url: string, signal?: AbortSignal): Promise<Response> {
  const response = await fetch(url, {
    credentials: 'same-origin',
    headers: view ? { 'X-Dispatch-View': view } : {},
    signal,
  });
  if (!response.ok)
    throw failed(
      response.status,
      await response.json().catch(() => ({})),
      response.headers.get('x-request-id') ?? undefined,
    );
  return response;
}

/** Saves what `url` answers as a file, under the name the server gives it. */
export async function download(url: string): Promise<void> {
  const response = await fetched(url);
  const link = document.createElement('a');
  link.href = URL.createObjectURL(await response.blob());
  link.download = savedName(response.headers.get('content-disposition')) ?? 'download';
  link.click();
  // The browser has the file once the click is handled; the address goes a moment after.
  setTimeout(() => URL.revokeObjectURL(link.href), 60_000);
}

/**
 * A picture `url` answers, for the page to show: an image's own address can't send the DSP's
 * view, so it comes as bytes the page shows at an address of its own.
 */
export async function picture(url: string, signal?: AbortSignal): Promise<Blob> {
  return (await fetched(url, signal)).blob();
}
