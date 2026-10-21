# User guide

Sources of the player's user guide (PDF). The output goes to `out/guide/tanks-user-guide.pdf` (gitignored, like every screenshot).

| File | What it is |
|---|---|
| `chapters/*.md` | The text, concatenated in file-name order: `00-cover`, `10-basics` (Part I), `20-advanced` (Part II), `30-strategy` (Part III), `40-reference` |
| `style.css` | Print stylesheet (A4, IBM Plex Mono headings, running headers, page numbers, table of contents with page numbers) |
| `build.py` | Markdown → HTML → PDF (WeasyPrint). Crops screenshots, draws callout pins, numbers chapters, resolves chapter references, draws formation diagrams from the doctrine packs |
| `build.sh` | Runs the build in a container; `--shots` retakes every screenshot first |
| `shots-menus.sh`, `shots-cli.sh` | Screenshots taken with the client's own scripting options (`--screen`, `--keys`, `--order`, `--plan`, `--advance` …), in the dev container |
| `shots-live.sh`, `drive-lib.sh` | Screenshots that need real mouse input (hover tooltips, held right click, Shift quasimode, tools), driven with xdotool on Xvfb |
| `containers/` | The two throwaway images: `localhost/tanks-guide` (WeasyPrint, python-markdown, Pillow, poppler) and `localhost/tanks-shots` (dev image + xdotool, scrot) |

```bash
docs/guide/build.sh            # PDF from the existing screenshots in out/guide/img (~10 s)
docs/guide/build.sh --shots    # retake all screenshots, then the PDF (~10 min)
tools/dev/devbox.sh run bash docs/guide/shots-cli.sh plan   # retake only shots whose name contains "plan"
```

Neither image is needed to play or develop; remove them with `podman rmi localhost/tanks-guide localhost/tanks-shots`.

## Markup

Plain Markdown (python-markdown `extra`) plus raw HTML, and these line or inline macros:

- `::fig <shot> | crop=x,y,w,h | pins=1@x,y;2@x,y | w=60 | Caption` — screenshot `out/guide/img/<shot>.png`, cropped in source pixels, with numbered callouts at source-pixel positions; `w` ≤ 50 puts figures side by side. Explain the pins in a table using `<span class="pinref">1</span>`.
- `::formations <pack>` — formation diagrams from `data/doctrine/<pack>/doctrine.ron`.
- `{{ch:Title}}` → "Chapter N" (linked) and `{{chn:Title}}` → "N" for the chapter whose title is or starts with the words. Use these instead of hard-coded numbers.
- `{{img:<shot>[:x,y,w,h]}}` — image URI (cover art).
- Boxes: `<div class="tip|note|warn|doctrine" markdown="1">` with an optional `<span class="label">…</span>`.

## Keeping it current

- When a control, label or hotkey changes, search the chapters for it and retake the affected screenshots. The texts quote the client exactly (button labels, radio lines, hint lines); the source of truth is `crates/client/src/game.rs` (card, tooltips, radio text), `game/ctx.rs` (context menu), `game/planedit.rs` (checkpoint menu), `game/posture.rs` (value lists) and `mapscreen.rs` (F7 help).
- Callout coordinates are in screenshot pixels: a layout change in the client moves them. Check every figure with pins after retaking screenshots.
- Numbers quoted from `data/` (delays, ranges, smoke durations, artillery timings) carry no automatic check; grep the chapters when tuning data.
