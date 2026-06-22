// Bounds for the editor|render split ratio, so neither pane can be dragged shut.
export const MIN_SPLIT = 0.15;
export const MAX_SPLIT = 0.85;

export function clampSplit(
  ratio: number,
  min = MIN_SPLIT,
  max = MAX_SPLIT,
): number {
  return Math.min(max, Math.max(min, ratio));
}

// The split ratio for a pointer at clientX over a container, clamped. Falls back
// to an even split when the container has no measured width (e.g. pre-layout).
export function splitFromPointer(
  clientX: number,
  bounds: { left: number; width: number },
): number {
  if (bounds.width <= 0) return 0.5;
  return clampSplit((clientX - bounds.left) / bounds.width);
}
