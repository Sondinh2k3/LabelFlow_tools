import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { m1 } from '../app/m1Strings';

export function PanelLayout({ children }: { children: ReactNode }) {
  const element = useRef<HTMLElement>(null);
  const [widths, setWidths] = useState<[number, number]>(() => {
    try {
      const stored: unknown = JSON.parse(
        localStorage.getItem('labelflow-panel-widths') ?? 'null',
      );
      if (
        Array.isArray(stored) &&
        stored.length === 2 &&
        stored.every((n) => typeof n === 'number' && n >= 180 && n <= 400)
      )
        return stored as [number, number];
    } catch {
      /* Use defaults when settings are absent or corrupt. */
    }
    return [260, 280];
  });
  useEffect(() => {
    localStorage.setItem('labelflow-panel-widths', JSON.stringify(widths));
  }, [widths]);
  function resize(index: number, delta: number) {
    setWidths((previous) => {
      const next: [number, number] = [...previous];
      next[index] = Math.max(
        180,
        Math.min(
          400,
          (element.current?.clientWidth ?? 1280) - 360 - previous[1 - index]!,
          previous[index]! + delta,
        ),
      );
      return next;
    });
  }
  return (
    <main
      ref={element}
      className="workspace"
      style={
        {
          '--left-panel': `${widths[0]}px`,
          '--right-panel': `${widths[1]}px`,
        } as CSSProperties
      }
    >
      {children}
      {[0, 1].map((index) => (
        <div
          key={index}
          className={`panel-resizer panel-resizer-${index}`}
          role="separator"
          aria-label={index ? m1.rightPanel : m1.leftPanel}
          aria-orientation="vertical"
          aria-valuenow={widths[index]}
          aria-valuemin={180}
          aria-valuemax={400}
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
              event.preventDefault();
              resize(
                index,
                (event.key === 'ArrowRight' ? 10 : -10) * (index ? -1 : 1),
              );
            }
          }}
          onPointerDown={(event) => {
            event.preventDefault();
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            if (event.currentTarget.hasPointerCapture(event.pointerId))
              resize(index, event.movementX * (index ? -1 : 1));
          }}
          onPointerUp={(event) => {
            if (event.currentTarget.hasPointerCapture(event.pointerId))
              event.currentTarget.releasePointerCapture(event.pointerId);
          }}
        />
      ))}
    </main>
  );
}
