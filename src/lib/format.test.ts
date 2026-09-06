import { describe, expect, it } from "vitest";

import {
  basename,
  colorForReduction,
  formatBytes,
  formatDuration,
  formatPercent,
  formatQuality,
  subtleForStatus,
  truncateMiddle,
} from "@/lib/format";

describe("formatBytes", () => {
  it("formatBytes(0) returns '0 B'", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("formatBytes formats bytes correctly", () => {
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.0 KiB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MiB");
    expect(formatBytes(1024 * 1024 * 1024)).toBe("1.0 GiB");
  });

  it("formatBytes handles negative values", () => {
    expect(formatBytes(-1024)).toBe("-1.0 KiB");
  });
});

describe("formatDuration", () => {
  it("formatDuration returns ms for values < 1s", () => {
    expect(formatDuration(0)).toBe("0 ms");
    expect(formatDuration(500)).toBe("500 ms");
  });

  it("formatDuration returns M:SS for >= 1s", () => {
    expect(formatDuration(1000)).toBe("0:01");
    expect(formatDuration(60_000)).toBe("1:00");
  });

  it("formatDuration returns H:MM:SS for >= 1h", () => {
    expect(formatDuration(3_602_000)).toBe("1:00:02");
  });
});

describe("formatPercent", () => {
  it("formatPercent returns 1 decimal", () => {
    expect(formatPercent(37.12)).toBe("37.1%");
  });

  it("formatPercent handles negatives", () => {
    expect(formatPercent(-12.3)).toBe("-12.3%");
  });
});

describe("subtleForStatus", () => {
  it("returns subtle background classes", () => {
    expect(subtleForStatus("Completed")).toContain("bg-success-subtle");
    expect(subtleForStatus("Failed")).toContain("bg-error-subtle");
    expect(subtleForStatus("Queued")).toContain("bg-muted");
  });
});

describe("colorForReduction", () => {
  it("returns success for >= 40%", () => {
    expect(colorForReduction(50)).toBe("text-success");
  });
  it("returns primary for >= 15%", () => {
    expect(colorForReduction(20)).toBe("text-primary");
  });
  it("returns muted-foreground for > 0%", () => {
    expect(colorForReduction(5)).toBe("text-muted-foreground");
  });
  it("returns disabled for <= 0%", () => {
    expect(colorForReduction(0)).toBe("text-disabled");
    expect(colorForReduction(-5)).toBe("text-disabled");
  });
});

describe("formatQuality", () => {
  it("returns '—' for NaN", () => {
    expect(formatQuality(Number.NaN)).toBe("—");
  });
  it("returns 3-digit pct for valid values", () => {
    expect(formatQuality(0.985)).toBe("099");
    expect(formatQuality(0.0)).toBe("000");
  });
});

describe("basename", () => {
  it("returns the last segment of a unix path", () => {
    expect(basename("/home/user/pictures/photo.png")).toBe("photo.png");
  });
  it("returns the last segment of a windows path", () => {
    expect(basename("C:\\Users\\user\\photo.png")).toBe("photo.png");
  });
  it("returns the input if it has no separator", () => {
    expect(basename("photo.png")).toBe("photo.png");
  });
});

describe("truncateMiddle", () => {
  it("returns the input if shorter than maxLen", () => {
    expect(truncateMiddle("short", 20)).toBe("short");
  });
  it("truncates with ellipsis in the middle", () => {
    const result = truncateMiddle("/very/long/path/to/file.png", 20);
    expect(result).toContain("…");
    expect(result.length).toBeLessThanOrEqual(20);
  });
});
