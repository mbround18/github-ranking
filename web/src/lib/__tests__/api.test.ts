import { describe, expect, it } from "vitest";
import { badgeUrl, RankError } from "../api";

// The frontend deliberately holds very little logic — ranking and rendering
// live in the wasm engine and are tested in Rust. What remains is URL
// construction and error classification, which is what these cover.

describe("badgeUrl", () => {
  const origin = "https://rank.example.com";

  it("omits the theme parameter for the default theme", () => {
    // A README badge should carry the shortest URL that works.
    expect(badgeUrl("octocat", "default", origin)).toBe(
      "https://rank.example.com/api/rank/octocat",
    );
  });

  it("includes any other theme", () => {
    expect(badgeUrl("octocat", "ocean", origin)).toBe(
      "https://rank.example.com/api/rank/octocat?theme=ocean",
    );
  });

  it("builds against the current origin by default", () => {
    expect(badgeUrl("octocat", "default")).toContain("/api/rank/octocat");
  });
});

describe("RankError", () => {
  const body = (code: number) => ({
    error: "UserNotFound",
    code,
    message: "User not found: ghost",
    requestId: "abc-123",
  });

  it("carries the server's message and request id", () => {
    const error = new RankError(404, body(404));

    expect(error.message).toBe("User not found: ghost");
    expect(error.body?.requestId).toBe("abc-123");
    expect(error).toBeInstanceOf(Error);
  });

  it("distinguishes a missing user from a real failure", () => {
    expect(new RankError(404, body(404)).isNotFound).toBe(true);
    expect(new RankError(429, body(429)).isNotFound).toBe(false);
    expect(new RankError(429, body(429)).isRateLimited).toBe(true);
    expect(new RankError(502, body(502)).isRateLimited).toBe(false);
  });

  it("falls back to a usable message when the body did not parse", () => {
    // A proxy can return something that is not our error envelope.
    const error = new RankError(502, null);

    expect(error.message).toContain("502");
    expect(error.isNotFound).toBe(false);
  });
});
