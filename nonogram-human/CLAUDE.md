# nonogram-human

Human-logic solver — the author's own implementation, written without AI assistance.

## AI Boundary

**Claude must not implement solving logic in this crate.**

The only things Claude may touch here:
- `Cargo.toml` — dependency wiring
- The `impl Solver for HumanSolver` stub — keeping the trait bound compiling
- This CLAUDE.md

Everything inside `HumanSolver::solve` (and any helper functions it calls) is written exclusively by the author. If the author asks Claude to review or discuss an approach, that is fine — but Claude must not write the implementation.

## Intent

This solver models the reasoning process a human uses when solving a nonogram by hand:
- Identifying forced cells through technique-by-technique deduction
- Tracking clue-to-block associations explicitly
- Using domain knowledge about which techniques apply to which line states

The `Cell` concept here may be richer than a 3-valued state: a cell may carry associations to its row clue, its column clue, candidate sets, or other metadata the author finds useful. Define whatever internal types serve that model.

## Public API

```rust
pub struct HumanSolver;
impl Solver for HumanSolver { ... }
```

The stub currently returns `Outcome::Stuck` for all inputs. Replace the body of `solve` when ready.

## Ownership

This crate is owned by the **human author (user)**. There is no Claude-Human agent.

**What Claude-Main may do here:**
- Make required superficial interface changes to keep the crate compiling against `nonogram-core`: updating imports, renaming fields in `SolveResult` literals, adapting trait method signatures. These changes are mechanical ripple effects of core changes and Claude-Main owns them as part of cross-cutting commit responsibility.
- Discuss approaches or review logic on request.

**What no Claude agent may do:**
- Touch any solving logic or internals. Everything inside `HumanSolver::solve` and any helpers it calls is written exclusively by the author.

The user monitors all discussions and is always aware of interface changes made here. No explicit ping is needed before Claude-Main makes a superficial interface fix.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` update, touches files in other crates, or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).

This rule applies even if the user explicitly asks this Claude to commit. Git has no technical barrier — any Claude can commit from any working directory in the repo — but a partial cross-boundary commit leaves the workspace broken. When in doubt, check scope first.
