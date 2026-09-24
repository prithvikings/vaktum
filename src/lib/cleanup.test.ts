import { describe, expect, it } from "vitest";
import { cleanupTranscript } from "./cleanup";

describe("cleanupTranscript", () => {
  it("normalizes whitespace", () => {
    expect(cleanupTranscript("   hello    world   ")).toBe("Hello world");
  });

  it("fixes punctuation spacing", () => {
    expect(cleanupTranscript("hello ,world")).toBe("Hello, world");
  });

  it("removes repeated adjacent words", () => {
    expect(cleanupTranscript("test test this")).toBe("Test this");
  });

  it("removes repeated adjacent words case-insensitively", () => {
    expect(cleanupTranscript("hello Hello HELLO world")).toBe("Hello world");
  });

  it("removes runs longer than two words", () => {
    expect(cleanupTranscript("go go go now")).toBe("Go now");
  });
});
