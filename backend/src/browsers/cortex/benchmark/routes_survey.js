// Surveys the routes page for the probe: its controls by kind, what appeared after
// a press, and pressing a route card or an unlabeled icon button. Only fixed
// interface words leave the page; any other text is reported as its length.
(input) => {
  const known =
    /^(Download CSV|Routes|DAs\/DPs|List|Map|All routes|All DAs|View legend|Contact|Clear search|Cortex FAQs|FAQs|Click here to view data for today|Desktop version|Toggle mobile menu|Home|Scheduling|Operations|Performance|Payments|Administration|Support|Work Summary Tool|Site Terms|Privacy Notice)$/i;
  const label = (value) => {
    const text = (value || '').trim();
    return known.test(text) ? text : text ? `(text:${text.length})` : '';
  };
  const visible = (e) => e.getClientRects().length > 0;
  const describe = (e) =>
    [
      e.tagName.toLowerCase(),
      e.getAttribute('role') || '',
      e.getAttribute('data-testid') || '',
      label(e.getAttribute('aria-label') || e.getAttribute('title') || ''),
      e.querySelector('svg') ? 'svg' : '',
      String(e.children.length),
      label(e.textContent),
    ].join(' | ');
  const all = (selector) => [...document.querySelectorAll(selector)];
  switch (input.action) {
    case 'links':
      return [
        ...new Set(
          all('a[href]')
            .map((a) => a.href)
            .filter((h) => h.startsWith(location.origin) && h.includes('/operations/execution/')),
        ),
      ].slice(0, 60);
    case 'controls':
      return all(
        input.selector ||
          'button,[role=button],[role=row],[role=link],a,select,input,tr,[data-testid]',
      )
        .filter(visible)
        .slice(0, 120)
        .map(describe);
    case 'downloadControls':
      return all('button,[role=button],a')
        .map((e) => label(e.getAttribute('aria-label') || e.getAttribute('title') || e.textContent))
        .filter((t) => /download|export|csv|xlsx|excel|spreadsheet/i.test(t))
        .slice(0, 20);
    case 'pressCard': {
      const list = document.querySelector('[data-testid=virtuoso-item-list]');
      if (!list) return 'no_list';
      const card = list.firstElementChild;
      if (!card) return 'no_card';
      const target = card.querySelector('a,button,[role=button],[role=link]') || card;
      target.click();
      return `clicked:${target.tagName}:${list.children.length}`;
    }
    case 'pressIcon': {
      const before = new Set(all('*').filter(visible));
      const icons = all('button,[role=button]').filter(
        (e) => visible(e) && e.querySelector('svg') && !(e.textContent || '').trim(),
      );
      const button = icons[input.index];
      if (!button) return { done: true, icons: icons.length };
      button.click();
      globalThis.__dispatchBefore = before;
      return { pressed: input.index, icons: icons.length };
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
          [e.tagName.toLowerCase(), e.getAttribute('role') || '', label(e.textContent)].join(' | '),
        )
        .filter((t) => t.split(' | ')[2])
        .slice(0, 25);
    case 'downloads':
      return (globalThis.__dispatchDownloads || []).map((d) => ({
        kind: d.kind,
        type: d.type,
        size: d.size,
        scheme: d.scheme,
        path: d.path,
      }));
    default:
      return 'unknown_action';
  }
};
