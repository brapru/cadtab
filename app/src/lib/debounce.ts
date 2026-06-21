// Calls `fn` once the caller stops invoking the returned function for `delayMs`,
// with the most recent arguments.
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  delayMs: number,
): (...args: A) => void {
  let handle: ReturnType<typeof setTimeout> | undefined;
  return (...args: A) => {
    if (handle !== undefined) clearTimeout(handle);
    handle = setTimeout(() => {
      handle = undefined;
      fn(...args);
    }, delayMs);
  };
}
