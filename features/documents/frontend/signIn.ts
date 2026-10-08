import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { connectGoogle } from '../api/client.js';

/** Sends the browser to Google to sign in: to connect Documents, or to reconnect it. */
export function useSignIn() {
  return useAction(async () => {
    const { url } = await connectGoogle();
    window.location.assign(url);
  });
}
