export function isTextEditing(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable ||
      ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
  );
}
export const shortcuts = { fit: '0', pan: ' ', reset: '=' } as const;
export const toolKeys = {
  v: 'select',
  b: 'bbox',
  p: 'polygon',
  m: 'brush',
  e: 'eraser',
  h: 'pan',
  s: 'sam',
} as const;
