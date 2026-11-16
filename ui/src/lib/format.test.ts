import { describe, expect, it } from "vitest";
import {
  buildWaveform,
  clamp,
  formatCountdown,
  formatDuration,
  hashString,
  progressPercent,
} from "./format";

describe("formatDuration", () => {
  it("formats minutes and seconds", () => {
    expect(formatDuration(0)).toBe("00:00");
    expect(formatDuration(4 * 60_000 + 5_000)).toBe("04:05");
    expect(formatDuration(3_723_000)).toBe("62:03");
  });

  it("clamps negative values", () => {
    expect(formatDuration(-5_000)).toBe("00:00");
  });
});

describe("formatCountdown", () => {
  it("uses a minus sign for remaining time", () => {
    expect(formatCountdown(138_000)).toBe("-02:18");
  });

  it("uses a plus sign for overtime", () => {
    expect(formatCountdown(-3_000)).toBe("+00:03");
  });
});

describe("progressPercent", () => {
  it("returns a clamped percentage", () => {
    expect(progressPercent(30, 120)).toBe(25);
    expect(progressPercent(-10, 120)).toBe(0);
    expect(progressPercent(200, 120)).toBe(100);
    expect(progressPercent(10, 0)).toBe(0);
  });
});

describe("clamp", () => {
  it("bounds values", () => {
    expect(clamp(5, 0, 10)).toBe(5);
    expect(clamp(-5, 0, 10)).toBe(0);
    expect(clamp(50, 0, 10)).toBe(10);
  });
});

describe("buildWaveform", () => {
  it("is deterministic for the same seed", () => {
    expect(buildWaveform("Radio Ga Ga")).toEqual(buildWaveform("Radio Ga Ga"));
  });

  it("differs between seeds and stays normalized", () => {
    const first = buildWaveform("A", 32);
    const second = buildWaveform("B", 32);
    expect(first).not.toEqual(second);
    expect(first).toHaveLength(32);
    expect(first.every((value) => value > 0 && value <= 1)).toBe(true);
  });
});

describe("hashString", () => {
  it("returns unsigned 32-bit values", () => {
    const hash = hashString("Queen");
    expect(hash).toBeGreaterThanOrEqual(0);
    expect(hash).toBeLessThanOrEqual(0xffffffff);
    expect(hashString("Queen")).toBe(hash);
  });
});
