// Run in the application's world on the day's list. Asks the application's own API
// action for each read, as its pages ask for what they show, so the application builds
// and signs each request itself; the collection stops every one before it leaves the
// browser and keeps only its address and signature. At most `lanes` reads are asked at
// once: each waits until its request is answered, which the collection does by failing
// it. An action that does not wait like that stops every lane, so requests can never pile
// up in the browser. Nothing is read from the application or its store but the action.
//
// `{action: "sign", reads, lanes}` answers whether the reads started: not without the
// store, nor while an earlier run's lanes still wait (`busy`), nor when the store's action
// did not wait (`store` found, not started). `{action: "add", reads}`
// gives a started run more reads. `{action: "stop"}` lets no lane start another read and
// answers how many lanes still wait.
(input) => {
  const run = window.__dispatchRoutes;
  if (input.action === 'stop') {
    if (run) run.stopped = true;
    return { running: run ? run.running : 0 };
  }
  if (input.action === 'add') {
    if (!run || run.stopped) return { started: false };
    run.reads.push(...input.reads);
    run.fill();
    return { started: !run.stopped };
  }
  if (run && run.running > 0) return { started: false, busy: true };
  const isStore = (v) =>
    v &&
    typeof v.dispatch === 'function' &&
    typeof v.getState === 'function' &&
    typeof v.subscribe === 'function';
  // The store the application's React tree is given, from the first elements' fibers up.
  const find = () => {
    let checked = 0;
    for (const element of document.querySelectorAll('body *')) {
      if (checked++ > 400) break;
      const key = Object.keys(element).find(
        (k) => k.startsWith('__reactFiber$') || k.startsWith('__reactInternalInstance$'),
      );
      if (!key) continue;
      for (let fiber = element[key], depth = 0; fiber && depth < 500; depth++) {
        const props = fiber.memoizedProps;
        if (props && isStore(props.store)) return props.store;
        if (props && props.value && isStore(props.value.store)) return props.value.store;
        fiber = fiber.return;
      }
    }
    return null;
  };
  const store = find();
  if (!store) return { started: false };
  const state = { running: 0, stopped: false, reads: input.reads.slice(), next: 0 };
  window.__dispatchRoutes = state;
  const lane = async () => {
    state.running++;
    try {
      while (!state.stopped && state.next < state.reads.length) {
        const read = state.reads[state.next++];
        const asked = store.dispatch({
          'Call API': {
            endpoint: read.endpoint,
            method: 'get',
            params: read.params,
            types: [
              'DISPATCH_ROUTES_REQUEST',
              'DISPATCH_ROUTES_SUCCESS',
              'DISPATCH_ROUTES_FAILURE',
            ],
            serializer: () => null,
          },
        });
        if (!asked || typeof asked.then !== 'function') {
          state.stopped = true;
          break;
        }
        await asked.then(
          () => {},
          () => {},
        );
      }
    } catch {
      state.stopped = true;
    } finally {
      state.running--;
    }
  };
  // Each lane counts itself before its first read, so this starts at most `lanes`.
  state.fill = () => {
    while (!state.stopped && state.running < input.lanes && state.next < state.reads.length) lane();
  };
  state.fill();
  // An action that did not wait has stopped every lane already.
  return { started: !state.stopped, store: true };
};
