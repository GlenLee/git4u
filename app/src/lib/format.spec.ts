import { describe, expect, it, vi } from "vitest";
import { formatBytes, relativeTime } from "@/lib/format";

describe("formatBytes", () => {
  it("uses binary units and drops noise", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(48 * 1024 * 1024)).toBe("48.0 MB");
    expect(formatBytes(150 * 1024 * 1024)).toBe("150 MB");
  });
});

describe("relativeTime", () => {
  it("buckets timestamps into human units", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-12T12:00:00Z"));
    const now = Math.floor(Date.now() / 1000);
    expect(relativeTime(now)).toBe("刚刚");
    expect(relativeTime(now - 120)).toBe("2 分钟前");
    expect(relativeTime(now - 7200)).toBe("2 小时前");
    expect(relativeTime(now - 3 * 86400)).toBe("3 天前");
    vi.useRealTimers();
  });
});
