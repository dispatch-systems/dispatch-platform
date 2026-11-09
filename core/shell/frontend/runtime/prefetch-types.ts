export type PrefetchReader = (signal: AbortSignal, priority: 'high' | 'low') => Promise<unknown>;
export type PrefetchRead = string | { url: string; read: PrefetchReader };
