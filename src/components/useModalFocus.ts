import { useEffect, useRef } from 'react';
/** Only the topmost modal owns focus, including nested operation reports. */
export function useModalFocus() {
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    const panel = ref.current;
    if (!panel) return;
    const previous = document.activeElement;
    const top = () =>
      [...document.querySelectorAll('[aria-modal="true"]')].at(-1) === panel;
    const controls = () =>
      [
        ...panel.querySelectorAll<HTMLElement>(
          'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),a[href],[tabindex="0"]',
        ),
      ].filter((el) => el.offsetParent !== null);
    const focus = () => (controls()[0] ?? panel).focus();
    panel.tabIndex = -1;
    focus();
    const contain = (event: FocusEvent) => {
      if (top() && !panel.contains(event.target as Node)) focus();
    };
    const key = (event: KeyboardEvent) => {
      if (event.key !== 'Tab' || !top()) return;
      const nodes = controls(),
        index = nodes.indexOf(document.activeElement as HTMLElement);
      if (
        !nodes.length ||
        (event.shiftKey ? index <= 0 : index === nodes.length - 1)
      ) {
        event.preventDefault();
        (event.shiftKey ? nodes.at(-1) : nodes[0])?.focus();
      }
    };
    document.addEventListener('keydown', key, true);
    document.addEventListener('focusin', contain);
    return () => {
      document.removeEventListener('keydown', key, true);
      document.removeEventListener('focusin', contain);
      if (previous instanceof HTMLElement && previous.isConnected)
        previous.focus();
    };
  }, []);
  return ref;
}
