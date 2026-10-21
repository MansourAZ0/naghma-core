// Live frame store for high-rate analysis frames.
//
// Frames arrive ~90 times per second. Canvas renderers read `store.frame`
// directly in their requestAnimationFrame loops; React components use
// `useLive(store, selector)`, which re-renders only when the selected value
// changes, so a stable note causes no React work.

import { useEffect, useRef, useState } from "react";

type Listener<F> = (frame: F | null) => void;

export class LiveStore<F> {
  frame: F | null = null;
  private listeners = new Set<Listener<F>>();

  push(frame: F): void {
    this.frame = frame;
    this.onPush(frame);
    for (const l of this.listeners) l(frame);
  }

  reset(): void {
    this.frame = null;
    this.onReset();
    for (const l of this.listeners) l(null);
  }

  subscribe(l: Listener<F>): () => void {
    this.listeners.add(l);
    return () => {
      this.listeners.delete(l);
    };
  }

  /** Hook for subclasses that derive extra state (history, event queues). */
  protected onPush(_frame: F): void {}

  /** Hook for subclasses to clear derived state. */
  protected onReset(): void {}
}

/**
 * Subscribe a component to a value derived from the latest frame (`null`
 * when there is none). Re-renders only when `equal(prev, next)` is false,
 * at most `maxHz` times per second, with a guaranteed trailing update.
 */
export function useLive<F, T>(
  store: LiveStore<F>,
  selector: (frame: F | null) => T,
  equal: (a: T, b: T) => boolean = Object.is,
  maxHz = 60,
): T {
  const sel = useRef(selector);
  sel.current = selector;
  const eq = useRef(equal);
  eq.current = equal;
  const [value, setValue] = useState<T>(() => selector(store.frame));
  const current = useRef(value);

  useEffect(() => {
    let last = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const minGap = 1000 / maxHz;
    const flush = () => {
      timer = null;
      last = performance.now();
      const next = sel.current(store.frame);
      if (!eq.current(current.current, next)) {
        current.current = next;
        setValue(next);
      }
    };
    const unsubscribe = store.subscribe(() => {
      const now = performance.now();
      if (now - last >= minGap) flush();
      else if (timer === null) timer = setTimeout(flush, minGap - (now - last));
    });
    return () => {
      unsubscribe();
      if (timer !== null) clearTimeout(timer);
    };
  }, [maxHz, store]);

  return value;
}

/** Shallow equality for flat objects. */
export function shallowEqual<T extends object | null>(a: T, b: T): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  const ra = a as Record<string, unknown>;
  const rb = b as Record<string, unknown>;
  const ka = Object.keys(ra);
  if (ka.length !== Object.keys(rb).length) return false;
  return ka.every((k) => Object.is(ra[k], rb[k]));
}
