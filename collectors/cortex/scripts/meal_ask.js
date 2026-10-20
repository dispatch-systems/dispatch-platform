// Run in the application's world on the day's list. Asks the application's own API action
// for one itinerary, as its pages ask for what they show, so the request is the
// application's own, built and signed by it, and the hook reads its answer as it reads a
// move's. Answers whether the action took it: not without the application's store, nor
// when the action did not wait on a request. Nothing is read from the application or its
// store but the action; the store found is kept for the next route.
(read) => {
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
  try {
    let store = window.__dispatchMealStore;
    if (!isStore(store)) {
      store = find();
      if (!store) return false;
      window.__dispatchMealStore = store;
    }
    const asked = store.dispatch({
      'Call API': {
        endpoint: read.endpoint,
        method: 'get',
        params: read.params,
        types: ['DISPATCH_MEALS_REQUEST', 'DISPATCH_MEALS_SUCCESS', 'DISPATCH_MEALS_FAILURE'],
        serializer: () => null,
      },
    });
    if (!asked || typeof asked.then !== 'function') return false;
    asked.then(
      () => {},
      () => {},
    );
    return true;
  } catch {
    return false;
  }
};
