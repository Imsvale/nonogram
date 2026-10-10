import { newGrid, type Grid } from "../core/types";
import type { SolveStep } from "../wasm/analyze";

const EMPTY_SET: ReadonlySet<number> = new Set();

/**
 * Read-only, step-by-step playback of a solve trace. Never touches `Game` — closing it
 * (or never opening one) can't affect the real puzzle grid, by construction.
 *
 * `index` runs from -1 (nothing applied yet — "start") to `steps.length - 1` ("end"). Every
 * cumulative grid is precomputed once up front, so stepping is O(1) rather than replaying —
 * cheap even for a trace with thousands of steps, and `open()`'s already-async (behind the
 * analyze worker round trip) so the one-time cost of building them is free.
 */
export class SolutionViewer {
  private readonly snapshots: Grid[];
  private index = -1;
  private playing = false;
  private timer: ReturnType<typeof setInterval> | undefined;
  /** Set by the caller; fired after every state change (index, play/pause). */
  onChange: () => void = () => {};

  constructor(
    private readonly steps: readonly SolveStep[],
    puzzle: { width: number; height: number },
    private speed: number,
  ) {
    this.snapshots = buildSnapshots(steps, puzzle);
  }

  get length(): number {
    return this.steps.length;
  }
  get position(): number {
    return this.index;
  }
  get atStart(): boolean {
    return this.index <= -1;
  }
  get atEnd(): boolean {
    return this.index >= this.steps.length - 1;
  }
  get grid(): Grid {
    return this.snapshots[this.index + 1];
  }
  get currentStep(): SolveStep | null {
    return this.index >= 0 ? this.steps[this.index] : null;
  }
  get previousStep(): SolveStep | null {
    return this.index >= 1 ? this.steps[this.index - 1] : null;
  }
  get nextStep(): SolveStep | null {
    return this.index + 1 < this.steps.length ? this.steps[this.index + 1] : null;
  }
  get highlight(): { current: ReadonlySet<number>; previous: ReadonlySet<number> } {
    return {
      current: this.currentStep ? new Set(this.currentStep.cellsChanged.map((c) => c.idx)) : EMPTY_SET,
      previous: this.previousStep ? new Set(this.previousStep.cellsChanged.map((c) => c.idx)) : EMPTY_SET,
    };
  }
  get isPlaying(): boolean {
    return this.playing;
  }

  next(): void {
    if (this.atEnd) {
      this.pause();
      return;
    }
    this.index++;
    this.onChange();
  }
  prev(): void {
    if (this.atStart) return;
    this.index--;
    this.onChange();
  }
  toStart(): void {
    if (this.atStart) return;
    this.pause();
    this.index = -1;
    this.onChange();
  }
  toEnd(): void {
    if (this.atEnd) return;
    this.pause();
    this.index = this.steps.length - 1;
    this.onChange();
  }

  togglePlay(): void {
    this.playing ? this.pause() : this.play();
  }
  play(): void {
    if (this.playing) return;
    if (this.atEnd) this.index = -1; // restart from the beginning
    this.playing = true;
    this.timer = setInterval(() => this.next(), 1000 / this.speed);
    this.onChange();
  }
  pause(): void {
    if (!this.playing) return;
    clearInterval(this.timer);
    this.timer = undefined;
    this.playing = false;
    this.onChange();
  }
  setSpeed(stepsPerSec: number): void {
    this.speed = stepsPerSec;
    if (this.playing) {
      clearInterval(this.timer);
      this.timer = setInterval(() => this.next(), 1000 / this.speed);
    }
  }

  /** Stop playback. Doesn't touch any grid — there's nothing to roll back. */
  close(): void {
    this.pause();
  }
}

function buildSnapshots(steps: readonly SolveStep[], puzzle: { width: number; height: number }): Grid[] {
  const out: Grid[] = [newGrid(puzzle)];
  for (const step of steps) {
    const g = out[out.length - 1].slice();
    for (const c of step.cellsChanged) g[c.idx] = c.state;
    out.push(g);
  }
  return out;
}
