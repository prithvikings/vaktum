import type { VaktumState } from "./types";

const transitions: Record<VaktumState, readonly VaktumState[]> = {
  idle: ["recording", "transcribing", "inserting"],
  recording: ["processing", "transcribing", "error"],
  processing: ["transcribing", "error"],
  transcribing: ["recording", "idle", "inserting", "error"],
  inserting: ["recording", "idle", "error"],
  error: ["idle"],
};

export function canTransition(from: VaktumState, to: VaktumState): boolean {
  return transitions[from].includes(to);
}

export function transition(
  from: VaktumState,
  to: VaktumState,
): VaktumState {
  if (!canTransition(from, to)) {
    throw new Error(`Invalid Vaktum transition: ${from} -> ${to}`);
  }

  return to;
}
