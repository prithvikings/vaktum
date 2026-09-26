import { describe, expect, it } from "vitest";
import { canTransition, transition } from "./state-machine";

describe("Vaktum state machine", () => {
  it("accepts the operational transitions", () => {
    expect(canTransition("idle", "recording")).toBe(true);
    expect(canTransition("recording", "processing")).toBe(true);
    expect(canTransition("recording", "transcribing")).toBe(true);
    expect(canTransition("processing", "transcribing")).toBe(true);
    expect(canTransition("transcribing", "recording")).toBe(true);
    expect(canTransition("transcribing", "inserting")).toBe(true);
    expect(canTransition("inserting", "idle")).toBe(true);
    expect(canTransition("recording", "error")).toBe(true);
    expect(canTransition("processing", "error")).toBe(true);
    expect(canTransition("transcribing", "error")).toBe(true);
    expect(canTransition("inserting", "error")).toBe(true);
    expect(canTransition("error", "idle")).toBe(true);
  });

  it("rejects transitions outside the runtime model", () => {
    expect(canTransition("idle", "transcribing")).toBe(true);
    expect(canTransition("idle", "inserting")).toBe(true);
    expect(canTransition("recording", "inserting")).toBe(false);
    expect(canTransition("error", "inserting")).toBe(false);
    expect(canTransition("inserting", "recording")).toBe(false);
  });

  it("returns the target state for valid transitions", () => {
    expect(transition("idle", "recording")).toBe("recording");
    expect(transition("inserting", "idle")).toBe("idle");
  });

  it("throws for invalid transitions", () => {
    expect(() => transition("recording", "inserting")).toThrow(
      "Invalid Vaktum transition: recording -> inserting",
    );
  });
});
