/** What a form's field holds, as text: a field left out, or a file, reads as nothing. */
export function field(form: FormData, name: string) {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}
