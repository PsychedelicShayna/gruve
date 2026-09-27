# Idea board — design contract (v1.2)

## Purpose

Externalize a person's thinking through voice-directed construction of a spatial scene. The human supplies meaning; a model — any model, fast or slow — translates it into short, predictable commands. The board must not limit the model to what was implemented: everything on it is built from a handful of primitives, and anything a model builds from them can be saved as a parameterized preset for a less capable model to reuse.

## Principles

1. **Few primitives, honest types.** A card is not a type; it is a rectangle with a text area inside. The driver may create a "card" in one command, but the board stores primitives, reports primitives and lets the driver edit primitives.
2. **Presets are composites plus parameters.** Built-in presets (card, code, dot, table, …) and user-defined presets expand the same way, into a group of primitives with stable child ids. Changing a parameter re-expands the group.
3. **The driver never needs a screenshot to know geometry.** Every object reports its resolved world box; every text reports its content size and whether it overflows. The human's camera is readable and driveable.
4. **Local coordinates, one parent.** Children are positioned relative to their parent group. Moving a group is one change. Presets are position-independent by construction.
5. **One spelling per intent.** Primitives and presets are both made with `create`; parameters are fields. Old scenes are upgraded on load; old vocabulary is not taught.

## Primitives

| type | fields | notes |
|---|---|---|
| `rect` | `w`,`h`,`rx`, style | rounded corners via `rx` |
| `ellipse` | `w`,`h` or `r`, style | a vertex is a small ellipse |
| `polygon` | `points:[[x,y],…]`, `smooth`, style | closed; a face |
| `polyline` | `points`, `smooth`, `head`, `tail`, style | open; heads at the ends |
| `text` | `w`,`h`, `text`, `size`, `font` (`sans`\|`mono`), `weight`, `align`, `color` | wraps at `w`; reports `measured` |
| `edge` | `from`, `to`, `route`, `curve`, `head`, `tail`, `label`, style, `rest`, `strength` | root-level connection that follows its endpoints |
| `group` | `children:[ids]`, `w`,`h`, `layout`, `padding`, `outline`, `title`, physics fields | the only container |

Common: `id`, `type`, `x`, `y` (relative to the parent; world for roots), `z` (reserved, ignored), `tags`, `opacity`, `pinned`, `body`, `mass`, `vx`, `vy`. Instances also carry `preset` and `params`. Read-only, browser-reported: `box` (world `{x,y,w,h}`, logical geometry excluding stroke) and, for text, `measured` (`{w,h,contentW,contentH,overflow}`).

Style: `color` (stroke / text), `fill`, `strokeWidth`, `dash` (`[on,off]`), `opacity`.

### Sizing — per axis

`w` and `h` are each a number, `"hug"` or `"fill"`.

- **Defaults.** `rect`/`ellipse`: numbers required unless `fill`. `text`: `w` 260, `h` hug. `group`: hug both.
- **hug**: the union of the children's boxes plus `padding` on that axis (text: its wrapped content). Children whose size on that axis is `fill` do not contribute.
- **fill**: the parent's inner box on that axis. A fill child that is the only child gives a hug parent size `2·padding`.

Resolution order is fixed: widths flow down (explicit → fill from parent → hug from children), text wraps at its resolved width and measures, heights flow up, then positions. Only text is measured, only in the visible browser, only after fonts are ready. Reports are tagged with the scene sequence; a report older than the current sequence is ignored.

### Layout

`layout:{"type":"stack"|"row"|"grid","gap":n,"cols":n,"align":"start"|"center"|"end"}` positions non-fill children in `children` order from `(padding,padding)` and ignores their `x`/`y`. Grid tracks are content-sized: a column is as wide as its widest cell, a row as tall as its tallest. Without `layout`, children sit at their own `x`/`y`; the group's `x`/`y` is its transform origin and its hug bounds may start negative — fill children follow the bounds, not the origin. `move` on a laid-out child is an error; change `children` order with `set` instead.

### Edges

`from` / `to`: an object id (attach on the outline toward the other end: exact for rect/ellipse/polygon, box for group/text), `{"id":"a","side":"left|right|top|bottom","offset":0..1}` (side midpoint by default), `{"id":"a","at":[fx,fy]}` (a point inside the box, fractions), `{"id":"poly","vertex":n}` (a polygon/polyline point, validated), or `[x,y]` (a fixed world point). Coincident centres attach at the centre. Routes: `straight`, `curve` (bend `curve` px; positive bends to the right of travel), `elbow` (axis-aligned, one bend, exits along the chosen or dominant axis). Heads: `none`, `arrow`, `open`, `dot`, `diamond`, `bar`. Labels sit at the route midpoint. Removing an endpoint removes the edge; re-expanding a preset keeps children with the same id, so edges to them survive.

## Presets

A preset is `{"name","params":{name:default…},"items":[…]}`. Items are primitive templates. Any string may interpolate `${param}` once (a substituted value is never re-scanned). A numeric field may be `{"$":"w - 30"}`: `+ - * / ( )`, numbers and parameters only. An item `{"repeat":"cells","as":"cell","index":"i","items":[…]}` expands its `items` once per element of the array parameter, with `${cell}` and `${i}` bound; repeats nest. Child ids must be unique after expansion. Expansion creates a `group` with `preset`, `params` and children `instance/child-id`.

Built-ins live in `presets.json` and use exactly this machinery: `card`, `code`, `label`, `dot`, `diamond`, `table`, `list`, `box` (titled outline around given children). Users add more with `define`; definitions persist and are listed in `state.presets` with their parameter schema.

`create {"type":"card","id":"a","title":"…"}` expands a preset when `type` names one; parameters are the remaining fields. `set` on an instance: keys that are parameters re-expand the instance; other keys apply to the group. Editing a child directly marks the instance `overridden`; a later parameter change is rejected unless `resetOverrides:true`, so a fix is never silently lost.

## Commands

`create`, `set` (null deletes a field), `remove`, `move` (`by`/`to` in parent coordinates), `reparent` (`select`, `into`; keeps world position), `group`, `ungroup`, `link` (edge sugar), `define`, `physics`, `impulse`, `layout` (arrange roots), `view`, `wait`, `clear`, `undo`, `redo`. Selection: id, list, or filters `type`, `tag`, `preset`, `parent`, `roots`, `fraction`, `slice`, `limit`.

## Compatibility

v1.1 scenes upgrade once on load (a backup is written first): `card`/`code`/`text` → `card`/`code`/`label` instances; `dot`→`dot`, `diamond`→`diamond`, `rectangle`→`rect`; `line`/`arrow`/`path` with points → `polyline`/`polygon`; `from`/`to` links → `edge`; group members → children in local coordinates. Root ids are preserved; if a generated child id collides, the upgrade fails loudly.

## Out of scope

Rotation of 2D objects, tilted text, hidden-surface removal beyond painter's sorting, physics in the 3D view, images, Markdown, rich text, obstacle-avoiding routes, nested physics, constraint solving beyond hug/fill/stack/row/grid.
