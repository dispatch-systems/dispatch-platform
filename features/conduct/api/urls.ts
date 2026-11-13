// Conduct's read addresses, apart from its client so the manifest loaded up front can warm
// them without loading the client.

/** Compact current standings and history totals. */
export const conductUrl = '/api/dsp/conduct';
/** How backing signed copies up is going. */
export const backupUrl = `${conductUrl}/backup`;
