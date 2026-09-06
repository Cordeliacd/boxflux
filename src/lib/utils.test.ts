import { describe, expect, it } from "vitest";

import { cn, hashString, sleep } from "@/lib/utils";

describe("cn", () => {
  it("combines class names", () => {
    expect(cn("foo", "bar")).toBe("foo bar");
  });
  it("removes falsy values", () => {
    expect(cn("foo", false, null, undefined, "bar")).toBe("foo bar");
  });
  it("dedupes conflicting Tailwind classes (tailwind-merge)", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
  });
});

describe("sleep", () => {
  it("resolves after the given delay", async () => {
    const start = Date.now();
    await sleep(50);
    expect(Date.now() - start).toBeGreaterThanOrEqual(40);
  });
});

describe("hashString", () => {
  it("returns a stable hash for the same input", () => {
    expect(hashString("hello")).toBe(hashString("hello"));
  });
  it("returns different hashes for different inputs", () => {
    expect(hashString("hello")).not.toBe(hashString("world"));
  });
});
