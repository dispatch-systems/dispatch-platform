// Surveys the Scheduling page for the probe: its controls, pressing one by its label, and
// what appeared after. Only fixed interface words leave the page: the schedule's rows are not
// a table the page marks, so any other text, as a person's name, is reported as its length.
(input) => {
  const known =
    /^(Download|Export|Today|Day|Week|Month|Previous|Next|Filter|Filters|Search|Clear|Columns|Settings|Publish|Unpublish|Copy|Shifts|Shifts & Availability|Rostered Work Blocks|Availability|Calendar|Calendar view|List view|Drivers|Associates|Service area|Home|Scheduling|Operations|Performance|Payments|Administration|Support|FAQs|Cortex FAQs|Site Terms|Privacy Notice|Toggle mobile menu|Desktop version)$/i;
  const visible = (e) => e.getClientRects().length > 0;
  const inGrid = (e) => !!e.closest('table,[role=grid],[role=table],[role=row],[role=cell]');
  const label = (e, value) => {
    const text = (value || '').trim().replace(/\s+/g, ' ');
    if (!text) return '';
    return known.test(text) ? text : `(text:${text.length})`;
  };
  const name = (e) => e.getAttribute('aria-label') || e.getAttribute('title') || e.textContent;
  const describe = (e) =>
    [
      e.tagName.toLowerCase(),
      e.getAttribute('role') || '',
      e.getAttribute('data-testid') || '',
      e.querySelector('svg') ? 'svg' : '',
      label(e, name(e)),
    ].join(' | ');
  const all = (selector) => [...document.querySelectorAll(selector)];
  const controls = 'button,[role=button],[role=menuitem],[role=option],[role=tab],a,select,input';
  switch (input.action) {
    case 'controls':
      return all(controls)
        .filter((e) => visible(e) && !inGrid(e))
        .slice(0, 80)
        .map(describe);
    case 'grid': {
      // The page's first table: how many rows it shows, and its header labels.
      const grid = document.querySelector('table,[role=grid],[role=table]');
      if (!grid) return 'no_grid';
      const headers = [...grid.querySelectorAll('th,[role=columnheader]')].map((h) =>
        label(h, h.textContent),
      );
      return {
        tag: grid.tagName.toLowerCase(),
        rows: grid.querySelectorAll('tr,[role=row]').length,
        headers: headers.slice(0, 20),
      };
    }
    case 'press': {
      const pattern = new RegExp(input.pattern, 'i');
      const found = all(controls).filter(
        (e) => visible(e) && !inGrid(e) && pattern.test((name(e) || '').trim()),
      );
      const target = found[input.index || 0];
      if (!target) return { pressed: false, matches: found.length };
      globalThis.__dispatchBefore = new Set(all('*').filter(visible));
      target.click();
      return { pressed: describe(target), matches: found.length };
    }
    case 'icons': {
      // Controls showing only an icon, outside the navigation, described by what names their
      // icon: class names, the icon's title or symbol. Nothing is pressed here.
      const icons = all('button,[role=button],a')
        .filter((e) => visible(e) && !e.closest('nav,header') && e.querySelector('svg,i,img'))
        .filter((e) => !(name(e) || '').trim())
        .slice(0, input.limit || 20);
      const words = (value) =>
        String(value || '')
          .replace(/[^A-Za-z_ -]/g, ' ')
          .replace(/\s+/g, ' ')
          .trim()
          .slice(0, 80);
      return icons.map((e, index) => {
        const icon = e.querySelector('svg,i,img');
        const use = icon.querySelector && icon.querySelector('use');
        return {
          index,
          tag: e.tagName.toLowerCase(),
          classes: words(
            e.className && e.className.baseVal !== undefined ? e.className.baseVal : e.className,
          ),
          icon: [
            icon.tagName.toLowerCase(),
            words(icon.getAttribute('class')),
            words(icon.getAttribute('data-icon')),
            words(
              icon.querySelector &&
                icon.querySelector('title') &&
                icon.querySelector('title').textContent,
            ),
            words(use && (use.getAttribute('href') || use.getAttribute('xlink:href'))),
            words(icon.getAttribute('alt')),
          ]
            .filter(Boolean)
            .join(' / '),
          testid: words(e.getAttribute('data-testid')),
          x: Math.round(e.getBoundingClientRect().x),
          y: Math.round(e.getBoundingClientRect().y),
        };
      });
    }
    case 'pressIcon': {
      // The icon `icons` listed at `index`, only when what names it reads as a download.
      const icons = all('button,[role=button],a')
        .filter((e) => visible(e) && !e.closest('nav,header') && e.querySelector('svg,i,img'))
        .filter((e) => !(name(e) || '').trim());
      const button = icons[input.index];
      if (!button) return { pressed: false };
      const text = [button.className, button.innerHTML].join(' ');
      if (!/download|export|excel|spreadsheet|xlsx/i.test(text))
        return { pressed: false, why: 'not_download' };
      globalThis.__dispatchBefore = new Set(all('*').filter(visible));
      button.click();
      return { pressed: input.index };
    }
    case 'appeared':
      return all('*')
        .filter(
          (e) =>
            visible(e) &&
            !(globalThis.__dispatchBefore || new Set()).has(e) &&
            e.children.length <= 2,
        )
        .map((e) =>
          [e.tagName.toLowerCase(), e.getAttribute('role') || '', label(e, e.textContent)].join(
            ' | ',
          ),
        )
        .filter((t) => t.split(' | ')[2])
        .slice(0, 40);
    case 'downloads':
      return (globalThis.__dispatchDownloads || []).map((d) => ({
        kind: d.kind,
        type: d.type,
        size: d.size,
        scheme: d.scheme,
        host: d.host,
        path: d.path,
        download: d.download,
        kept: !!(d.base64 || d.dataUrl),
      }));
    case 'blob': {
      const d = (globalThis.__dispatchDownloads || [])[input.index];
      return d ? d.base64 || (d.dataUrl || '').split(',')[1] || '' : '';
    }
    default:
      return 'unknown_action';
  }
};
