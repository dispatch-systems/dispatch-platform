/** A number New message can offer to text, in the forms the phone API's numbers::normal accepts.
 * The server validates it again when opening the conversation. */
export function textNumber(input: string): string | undefined {
  const raw = input.trim();
  if (!raw || raw.length > 32 || !/^[0-9 +().-]+$/.test(raw)) return undefined;
  const plus = raw.startsWith('+');
  const digits = raw.replace(/\D/g, '');
  if (digits.length === 10 && !plus) return `+1${digits}`;
  if (digits.length === 11 && digits.startsWith('1')) return `+${digits}`;
  if (plus && digits.length >= 8 && digits.length <= 15) return `+${digits}`;
  if (/^[0-9]{3,7}$/.test(raw)) return digits;
  return undefined;
}
