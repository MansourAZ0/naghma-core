import { describe, expect, it } from "vitest";
import { LiveStore, shallowEqual } from "./live";

class CountingStore extends LiveStore<number> {
  pushes = 0;
  protected onPush(): void {
    this.pushes++;
  }
  protected onReset(): void {
    this.pushes = 0;
  }
}

describe("LiveStore", () => {
  it("keeps the latest frame, notifies listeners and runs hooks", () => {
    const s = new CountingStore();
    const seen: (number | null)[] = [];
    const off = s.subscribe((f) => seen.push(f));
    s.push(1);
    s.push(2);
    expect(s.frame).toBe(2);
    expect(s.pushes).toBe(2);
    s.reset();
    expect(s.frame).toBeNull();
    expect(s.pushes).toBe(0);
    off();
    s.push(3);
    expect(seen).toEqual([1, 2, null]);
  });
});

describe("shallowEqual", () => {
  it("compares flat objects", () => {
    expect(shallowEqual({ a: 1, b: "x" }, { a: 1, b: "x" })).toBe(true);
    expect(shallowEqual({ a: 1 }, { a: 2 })).toBe(false);
    expect(shallowEqual(null, null)).toBe(true);
    expect(shallowEqual({ a: 1 }, null)).toBe(false);
  });
});
