# nonogram-web

Browser version of the manual solver, deployed to https://imsvale.github.io/nonogram/.
TypeScript + Vite, no framework. **Not a Cargo crate**; it lives in the workspace
directory but is built with npm. Manual solving only — no machine solvers (yet).

```
npm install
npm run dev        # dev server on http://127.0.0.1:5173/ (does NOT open a browser)
npm test           # vitest (core logic, game model, geometry)
npm run build      # tsc --noEmit + vite build → dist/
```

`base` is `./`, so the build works at any sub-path. CI: `.github/workflows/deploy-web.yml`
(needs Settings → Pages → Source: GitHub Actions, once).

## Layout

| Path | Role |
|---|---|
| `src/core/` | Pure logic, no DOM. `parse` (native + Puz-Pre v3), `puzzlink` (codec), `share` (URL hash), `export`, `lines` (fulfilled / forced-empty / validity), `samples` |
| `src/state/` | `game.ts` (grid, strokes, undo, trial tiers, timer, solve detection), `settings.ts`, `progress.ts` (localStorage) |
| `src/ui/` | `geometry` (layout + hit-testing, pure), `render` (canvas), `gridview` (viewport + input), `app` (DOM chrome), `settingsPanel` |

`core/lines.ts` and `state/game.ts` port the logic of `nonogram-gui` (`grid_view.rs`,
`app/mod.rs`). Deliberate deviation: `forcedEmptyFromEdges` also fires when edge-confirmed
runs on only one side cover every clue (the GUI requires both sides).

## URLs

`#nonogram/W/H/DATA[?name=…&answer=…]` — DATA is the puzz.link clue encoding (`specs/puzzlink.md`).
Also accepted: `#s=<url-encoded native line>` and `?nonogram/W/H/DATA`. Pasted puzz.link URLs,
native-format text and Puz-Pre v3 are accepted by the import dialog, paste and drag-and-drop.
The address bar is always rewritten to the canonical share form when a puzzle opens.

## Gotchas

- Anything that appears/disappears **in flow** while playing shifts the canvas under the pointer
  and mis-paints the rest of a drag (this bit the "Solved" banner). Keep such UI as overlays.
- The puzz.link decoder tries strict first, then tolerates an explicit `f` after a full group,
  because that `f` is ambiguous with a following group starting with the value 15.
- puzzle-nonograms.com URL import from the desktop app is not ported: browsers block that
  cross-origin fetch (CORS).
- `window.__nonogram` exposes layout/game/settings for browser tests.

## Persistence (localStorage, this browser only)

- `nonogram-web:settings:v1` — all settings; defaults live in `src/state/settings.ts`
  (`defaultSettings()`, palettes `LIGHT` / `DARK`). Flushed on page hide.
- `nonogram-web:progress:v1` — per-puzzle grid, trial tiers, dimmed clues, elapsed time; keyed by a
  hash of the clues; capped at the 40 most recent puzzles.
- `nonogram-web:last:v1` — id of the last-opened puzzle; the start page offers Resume when it has
  progress and was never solved.

## Where the look is defined

Grid colours `src/state/settings.ts` · trial-tier colours and hover tints `src/ui/render.ts` ·
page chrome (buttons, header, drawer) CSS variables at the top of `src/style.css` ·
line thickness threshold `HEAVY_LINE_MIN_CELL` in `src/ui/geometry.ts` · zoom limits in `src/ui/gridview.ts`.
