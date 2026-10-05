import { Globe, Laptop, Plug, type LucideIcon } from 'lucide-react';
import chatgpt from './assets/chatgpt.png?url';
import claude from './assets/claude.png?url';
import codex from './assets/codex.png?url';
import hermesDark from './assets/hermes-dark.png?url';
import hermesLight from './assets/hermes-light.png?url';

/** Each known app's own logo, by the kind of app it is: one for both themes, or a light
 * theme's and a dark theme's. */
const logos: Record<string, [string, string?]> = {
  chatgpt: [chatgpt],
  codex: [codex],
  'claude-code': [claude],
  hermes: [hermesLight, hermesDark],
};
/** Logos that aren't squares, drawn whole rather than filling theirs. */
const shaped = new Set(['codex']);
/** The kinds of app with no logo of their own. */
const icons: Record<string, LucideIcon> = { local: Laptop, web: Globe };

/** A kind of app as a small square: its logo when it is a known app, otherwise a plain icon.
 * `large` in the app picker. Decorative: the app's name is always written beside it. */
export function AppIcon({ app, large = false }: { app: string | null; large?: boolean }) {
  const size = large ? 44 : 28;
  const logo = app ? logos[app] : undefined;
  if (app && logo) {
    const [light, dark] = logo;
    const kind = `agents-logo${large ? ' large' : ''}${shaped.has(app) ? ' shaped' : ''}`;
    const image = (src: string, theme = '') => (
      <img
        className={`${kind}${theme}`}
        src={src}
        alt=""
        width={size}
        height={size}
        decoding="async"
      />
    );
    return dark ? (
      <>
        {image(light, ' logo-light')}
        {image(dark, ' logo-dark')}
      </>
    ) : (
      image(light)
    );
  }
  const Icon = (app && icons[app]) || Plug;
  return (
    <span className={`dsp-avatar agents-icon tone-0${large ? ' large' : ''}`} aria-hidden="true">
      <Icon size={large ? 22 : 15} />
    </span>
  );
}
