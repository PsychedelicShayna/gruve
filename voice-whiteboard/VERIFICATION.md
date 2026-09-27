# Verification, 2026-09-27 (adversarial review fixes)

- Reviews: one static review, one runtime probe (Grok 4.7, headless Chromium on an isolated server) and one review of the fix diff. The confirmed findings are fixed in model, server and browser commits.
- 39 Python model tests. The 13 first-round regression tests fail on the pre-fix model (12 by assertion; the nested-repeat cap test runs for more than 300 s there). They cover nested preset overrides, moved preset children, group props impersonating a preset, the group origin and world z through group, ungroup and reparent, reparenting out of a layout by reported origin, reserved built-in names, the re-expansion id collision, the expansion cap, preset group shells, `url()` paint, and malformed arguments. Four more tests come from the second review. A reported layout origin now follows a parent that moved after the report. Ungrouping inside a layout is refused. Re-expansion keeps reported boxes. Grouping or reparenting inside a preset marks the instance overridden.
- 20 JavaScript engine and camera tests, including a `w:"fill"` grid cell that keeps its column.
- Server smoke on an isolated port. NaN in the body, a string or out-of-range duration on undo/redo, non-object `/marks` bodies, and 100000-deep JSON each return 400. A rejected `/ack` leaves the scene unchanged. 800 events streamed over SSE arrive contiguous, and receipts are capped at 200.
- 22 of 22 browser sequence assertions pass (`--test` server, isolated data), three consecutive runs in fresh tabs. One run in a reused, frozen headless tab failed the timing-based progressive-removal check.
- Double-click editor in headless Chromium. An overridden card with no title child refuses a new title with a toast and stays open, keeping the typed text. An overridden card with title and body children saves through the children: the body colour override and `overridden` are kept.
- Visual check in headless Chromium: a titled outline group of two linked cards plus a cube. `{"op":"view","yaw":30,"pitch":-25}` without `mode` enters 3D. The group outline and title stay drawn and the edge is depth-sorted. No console errors.
- The live v1.1 scene upgrades to the same 120 objects as before the upgrade rewrite.

# Verification, 2026-09-22 (v1.2 independent QA)

- Independent QA (Xvfb + Helium, isolated server, the operator's real scene copied in): 14 of 15 checks passed — upgrade, card semantics, override protection, table, built-in and user presets, all edge anchors and routes, group drag/reparent/ungroup, layout rejection and reorder, marks, camera persistence across reload, physics, undo/redo across re-expansion, a 40-command request. The one failure — Ctrl-click selected a card's text child in state but drew no feedback, since text has no stroke to highlight — is fixed with an explicit dashed selection outline on every selected primitive; verified by real Ctrl-click on the bench. 22 browser sequence assertions re-run: ALL PASS.

# Verification, 2026-09-22 (v1.3, 3D view)

- 6 camera tests: identity at rest (2D/3D agreement), perspective scale and depth ordering, yaw/pitch axis swings, unproject inverting project at arbitrary orientation, orthonormal basis, shading and colour mixing.
- On a private Xephyr display with the real Helium (slice-capped, tiled on the operator's left monitor): `view mode:"3d"` with yaw/pitch drew shaded cube and pyramid faces, a far cube smaller and behind, billboarded cards, projected edges including fixed-point anchors; `state.view` reported `mode/yaw/pitch/cz`. Real right-drag orbited (yaw 35→107); real left-drag moved the front pyramid on the camera-facing plane by a bounded (+46, −105, +61) for an 80×120 px drag and the `move … to:[x,y,z]` reached the server; the idle checkpoint now carries z.
- Painter's sorting is per face by mean depth; intersecting faces can mis-sort. Occlusion between separate solids was correct in the runs above.

# Verification, 2026-09-22 (v1.2, primitives and composites)

- 22 Python model tests: preset expansion into stable child ids, single-item aliases, parameter re-expansion keeping edges, override protection with `resetOverrides`, `when`, user presets with expressions and nested repeat, literal `${}` text not reinterpreted, expression language rejecting anything beyond `+ - * / ( )`, table nesting, group/ungroup/reparent coordinate conversion, laid-out child move rejection, cascade removal, anchor validation including vertex references on later polygon edits, selection filters, null deletes, sizing keywords, v2→v3 upgrade.
- 13 JavaScript engine tests: card hug + fill background, text overflow, group overflow, negative-bounds hug with preserved origin, content-sized grid tracks with stretched cells, row layout, outline attachment on rect/ellipse/polygon, side/at/vertex/fixed anchors, elbow shape, head trims, root-only physics dragging resolved boxes, pinning/collision, interpolation of sizing keywords.
- 22 browser sequence assertions (`--test` server on 8794, isolated data): the v1.1 sequence plus card-as-group, hug/fill, parameter re-expansion growth, fixed-height overflow, table row stretching, and browser-resolved boxes/measurements reaching `/state`.
- The live v1.1 scene (51 objects) upgraded to 120 primitives with ids preserved and rendered without console errors; cards, constellation, zodiac grid and arrows intact. `data/scene.v2.json` is written before an upgrade.
- Headless smoke of the new vocabulary: table with a wrapping cell, list, mono code, user-defined pyramid, titled free group with a reparented dot, edges to a side with offset, from a vertex with a negative curve, and to a fixed point with `open`/`bar` ends.

# Verification, 2026-09-22 (v1.1)

- 17 Python model tests (+3): `view` targets and fit selection, link `head`/`route` validation, `set` null-deletes and read-only `measured`.
- 14 JavaScript engine tests (+5): measured-height bounds, boundary edge points on rectangles and ellipses, elbow routing shape, head trims.
- Headless Chromium against an isolated server on port 8791 with `/tmp/vwtest` data:
  - Six head styles and three routes render; links start/end on shape boundaries and re-route when an endpoint is resized.
  - A card without `height` grew to 205 px for its text; a 90 px card holding 163 px of text was outlined red and reported `measured.overflow:true` in `/state`.
  - Corner-handle drag resized a card 260×90 → 296×197, persisted through `set`, and cleared the overflow flag; `set height:null` returned it to auto-height (measured h 163).
  - Point mode: three clicks produced marks 1 (target `a`), 2 (target `c`), 3 (free, note "here"); `boardctl.py marks` returned and consumed them; the overlay dimmed them.
  - `view center/zoom` and `view fit:[ids]` moved the camera; `/state.view` reported the visible client's world rectangle.
- Limitation noted: the harness's idle headless tab stops `requestAnimationFrame`, so interactive checks must run within one live navigation. The board itself is unaffected.

# Verification, 2026-09-20

## Completed

- 14 Python model tests: selectors, fractional deletion, single-translation groups, cascading link removal, templates, validation, graph ownership and endpoint cycles.
- 9 JavaScript engine tests: compound translation, pinned bodies, impulses, springs, separation on collision, coincident-body stability, color interpolation and connector-aware compound bounds.
- 16 browser integration assertions in an isolated Chromium/Helium process against a separate server/data directory. Result and timing receipts: `browser-report.json`.
- Repeated all 16 browser assertions after separating idle position checkpoints from completion receipts; all passed. Final run: `browser-report-final.json`.
- Rendered and inspected the board at 1440×1000; saved `board-preview.png`.
- Restarted the isolated service and compared complete `/state` response SHA256 before/after: both `8f85a1e338179b1059aebd4722662917ce65fa4d536cc817dd61549d55821e5b`.
- Fresh independent read-only review of the implementation and driver contract. Reviewer executed the documented pyramid template against an isolated model successfully. Findings about mutable queued events, moving-object updates, empty semantic diffs, connector bounds and acknowledgments were corrected.
- Real visible browser: physics enable → pyramid impulse → 900 ms wait → pause completed in 1032.7 ms from sequence acceptance. The impulse's first frame acknowledgment arrived at 69.7 ms. Motion is paused at handoff.

## Browser assertions

Progressive removal has intermediate counts. Survivors remain blue while other dots disappear. Exactly half are removed, then survivors turn red. Their DOM object identity is unchanged. Earlier queued creation retains its original color. Compound motion preserves member offsets. Invalid sequences reject atomically. Text stays literal. Moving back to a server-unchanged position still acts after browser physics. Recoloring does not freeze motion. Undo/redo restore scene changes. Reconnect restores a snapshot without replaying animations. First-frame and completion receipts are recorded.

## Timing interpretation

The saved browser run recorded deletion's first frame at 28.7 ms and completion at 378.7 ms. The following recolor completed at 578.7 ms from acceptance of the entire sequence. A 300 ms compound move completed at 355.6 ms, including the preceding group command.

These are local server-acceptance to browser frame-callback measurements. They include intended animation duration and queue waiting. They do not measure speech endpointing, model reasoning, tool scheduling or a human's visual perception. The three-second whole-interaction target cannot be guaranteed by these measurements alone.

## Limits

Single-user, one controlling visible tab. Other visible tabs simulate independently. Collision uses axis-aligned boxes; no rotation or exact polygon dynamics. Physics is quadratic in body count and has been exercised with a modest scene, not benchmarked at the 1000-object registry limit. Text/code needs enough allocated card space; no rich embeds or automatic syntax validation. Voice recognition remains in the conversation, not the page. This is a working reusable prototype, not a general physics or diagramming application replacement.
