# Drive the idea board

The human supplies meaning. Translate their request into the smallest suitable commands and send promptly. The browser owns animation, layout and physics. You need this contract and object ids, not the implementation.

## Start and inspect

```sh
python3 /home/shayna/voice-whiteboard/boardctl.py start
python3 /home/shayna/voice-whiteboard/boardctl.py open
python3 /home/shayna/voice-whiteboard/boardctl.py state
python3 /home/shayna/voice-whiteboard/boardctl.py marks
```

The page is http://127.0.0.1:8770. Start is idempotent; open once. `state` returns `objects` by id, `presets` (name → parameters and doc), `physics`, `seq`, the human's `view` and unread `marks`. Coordinates are world pixels: x rightward, y downward. An object's `x`,`y` is its origin in its parent's frame (world for roots).

## Send and confirm

```sh
python3 /home/shayna/voice-whiteboard/boardctl.py send '{"op":"create","object":{"id":"idea","type":"card","x":-200,"y":0,"title":"An idea","text":"Its definition."}}' --wait
```

JSON can also arrive on stdin. HTTP: POST `/commands`. Send one command, an array, or `{"requestId":"…","commands":[…]}` to deduplicate retries. A whole array is validated before any of it applies, then executes in order. `--wait` waits up to 10 s for the browser to finish; HTTP success alone means accepted. Errors name the field and what was expected — read them and resend.

## Know what the human sees

- `state.view` — `{"cx","cy","zoom","w","h"}`: the visible tab's camera in world units. Outside `cx±w/2`, `cy±h/2` is off-screen for them. Move it with `view`.
- `objects[id].box` — `{"x","y","w","h"}` world box of every object as the browser actually laid it out, refreshed whenever the board is idle. This is where things *are*, including children of groups and text that grew.
- `objects[id].measured` — on text: `{"w","h","contentW","contentH","overflow"}`. On a group with an explicit size: whether its children spill out. `overflow:true` is also drawn as a red dashed outline. Cards with no fixed size never overflow; they grow.

You never need a screenshot to know whether things fit or overlap: compare boxes.

## Read the human's marks

In Point mode (button, `M`, or Alt-click) each click leaves a numbered mark — 1, 2, 3 — with an optional typed note, on an object or empty space. `boardctl.py marks` returns and consumes them (`--peek` looks only). Each: `{"n","batch","x","y","target":"id"|null,"note"}`; `target` is the root object under the click. Resolve "link 1 and 2", "move these to 3", "put a table here". Numbering restarts at 1 for each new batch; old batches stay visible but dimmed.

## Primitives

Everything on the board is one of seven primitives. Presets (below) are shorthand that expands into them.

| type | fields |
|---|---|
| `rect` | `w`, `h`, `rx` (corner radius) |
| `ellipse` | `w`, `h` or `r` |
| `polygon` | `points:[[x,y],…]` (closed), `smooth` |
| `polyline` | `points`, `smooth`, `head`, `tail` |
| `text` | `text`, `w` (wrap width, default 260), `h`, `size`, `font` (`sans`/`mono`), `weight`, `align` |
| `edge` | `from`, `to`, `route`, `curve`, `head`, `tail`, `label` |
| `group` | `children`, `w`, `h`, `layout`, `padding`, `title`, `outline` |

Common: `id`, `type`, `x`, `y`, `tags`, `opacity`, `color` (stroke/text), `fill`, `strokeWidth`, `dash:[on,off]`, physics `body`, `pinned`, `mass`.

### Sizing

`w` and `h` are numbers, `"hug"` (wrap the children / the wrapped text) or `"fill"` (take the parent's size). Groups hug by default; text hugs its height. A child with `w:"fill"` and `h:"fill"` is an overlay — it covers the group's whole box and does not affect its size (that is how a card's background works). `layout:{"type":"stack"|"row"|"grid","gap":n,"cols":n,"align":"start|center|end"}` positions children in `children` order; without a layout they sit at their own `x`,`y`. In a grid, columns are as wide as their widest cell and rows as tall as their tallest; `h:"fill"` cells stretch to the row.

Resize a card by setting its `w`; the text re-wraps and the height follows. `{"op":"set","props":{"h":null}}` removes a fixed height (null deletes any field).

### Edges

`from` / `to` accept:

- `"id"` — attach on the shape's outline, toward the other end;
- `{"id":"a","side":"left|right|top|bottom","offset":0..1}` — a point on a side (midpoint by default);
- `{"id":"a","at":[fx,fy]}` — a point inside the box by fractions;
- `{"id":"poly","vertex":2}` — a polygon/polyline point;
- `[x,y]` — a fixed world point.

`route`: `straight` (default), `curve` (`curve` = bend in px, negative bends the other way), `elbow` (one axis-aligned bend). `head` (at `to`) and `tail` (at `from`): `none`, `arrow`, `open`, `diamond`, `dot`, `bar`. Dependency `{"head":"arrow"}`; both ways `{"head":"arrow","tail":"arrow"}`; composition `{"tail":"diamond"}`; inhibit `{"head":"bar"}`. Edges are roots, can attach to anything except other edges (children of groups included), follow their endpoints and disappear with them. `label` sits at the midpoint.

## Presets

A preset is a named composite with parameters. Create one like a primitive; its parameters are its fields:

```json
{"op":"create","object":{"id":"a","type":"card","x":0,"y":0,"title":"Router","text":"Decides where a request goes."}}
{"op":"create","object":{"id":"t","type":"table","x":400,"y":0,"rows":[["Sign","Element"],["Aries","Fire"]],"cols":2,"cellW":140}}
{"op":"create","object":{"id":"l","type":"list","title":"Fates","items":["route","proxy","terminate"]}}
```

Built-ins (see `state.presets` for exact parameters and defaults): `card(title,text,w,color,fill,size)`, `code(title,text,w,…)` (monospace), `label(text,w,color,size,align)` (no box), `dot(r,color)`, `diamond(w,h,color,fill)`, `list(title,items,w,…)`, `table(rows,cols,cellW,color,fill,size)`.

An instance is a group whose children have stable ids `instance/child` (a card: `a/bg`, `a/title`, `a/body`). `set` on the instance with parameter keys re-expands it (`{"op":"set","select":"a","props":{"text":"…","color":"#ff596b"}}`); other keys apply to the group. You may edit children directly (`set select:"a/body" props:{color:…}`); after that, a parameter change is refused until you pass `"resetOverrides":true`, so a fix is never lost silently. `{"type":"card"}` in a selection filter matches card instances.

### Define your own

```json
{"op":"define","preset":{"name":"pyramid","params":{"size":100,"color":"#ffd479"},"items":[
  {"id":"outline","type":"polygon","points":[[0,{"$":"-size"}],[{"$":"-size*0.9"},{"$":"size*0.55"}],[{"$":"size*0.9"},{"$":"size*0.55"}]],"color":"${color}","fill":"none","strokeWidth":3},
  {"id":"ridge","type":"polyline","points":[[0,{"$":"-size"}],[0,{"$":"size*0.2"}]],"color":"${color}","strokeWidth":3}
]}}
{"op":"create","object":{"id":"p1","type":"pyramid","x":300,"y":0,"size":60}}
```

Rules: `${name}` substitutes a parameter into a string (a whole-string `"${points}"` passes an array through); `{"$":"expr"}` computes a number from `+ - * / ( )` and parameters; `{"repeat":"items","as":"item","index":"i","items":[…]}` expands its items once per element of an array parameter (`${item}`, `${i}`; repeats nest); `"when":"title"` skips an item when that parameter is empty; an item may name an earlier group item as `"parent"` to nest. Child ids must be unique. A preset with one item is an alias for that primitive (no wrapper group). An optional `"group":{…}` sets fields of the wrapper (`w`, `layout`, `padding`). Definitions persist with the scene. The built-ins are written in exactly this language — read `presets.json` for the card and table.

## Commands

| Intent | Example |
|---|---|
| Create | `{"op":"create","object":{"id":"a","type":"dot","x":0,"y":0}}` |
| Create several | `{"op":"create","items":[{…},{…}]}` |
| Create many | `{"op":"create","object":{"id":"dot","type":"dot","tags":["test"]},"count":100,"spread":350,"seed":4,"stagger":8}` |
| Edit | `{"op":"set","select":"a","props":{"color":"#ff596b","text":"Updated"}}` |
| Remove | `{"op":"remove","select":{"type":"dot","fraction":0.5},"duration":150,"stagger":15}` |
| Move | `{"op":"move","select":"a","by":[200,0],"duration":600}` / `"to":[0,0]` (one object) |
| Connect | `{"op":"link","id":"ab","from":"a","to":"b","arrow":true,"props":{"label":"requires","route":"curve"}}` |
| Group | `{"op":"group","id":"box","select":["a","b"],"props":{"title":"Reconcile these","color":"#ff596b"}}` |
| Ungroup | `{"op":"ungroup","select":"box"}` |
| Put into / take out of a group | `{"op":"reparent","select":"a","into":"box"}` / `"into":null` |
| Reorder children | `{"op":"set","select":"box","props":{"children":["b","a"]}}` |
| Pin / throw | `{"op":"set","select":"a","props":{"pinned":true}}` / `{"op":"impulse","select":"a","velocity":[700,0]}` |
| Physics | `{"op":"physics","props":{"enabled":true,"repulsion":1200,"center":0.02,"damping":0.9,"collision":true,"bounce":0.45}}` |
| Arrange roots | `{"op":"layout","select":{"roots":true},"mode":"grid","spacing":300,"duration":500}` |
| Camera | `{"op":"view","fit":true}` · `{"op":"view","fit":["a","b"]}` · `{"op":"view","center":[400,-120],"zoom":1.2,"duration":400}` · `{"op":"view","by":[300,0]}` |
| Wait / undo / redo | `{"op":"wait","duration":200}` · `{"op":"undo"}` · `{"op":"redo"}` |

## 3D

The board is 2D until the camera is unlocked: `{"op":"view","mode":"3d","yaw":35,"pitch":-30}` (or the **3D** button). Then:

- Every position may carry `z` (toward the viewer; default 0) and polygon/polyline points may be `[x,y,z]`. A `polygon` with 3D points is a **face**, flat-shaded and depth-sorted; a `polyline` is an **edge**; a small `ellipse` is a **vertex**. A free group (no `layout`) is a 3D container: its children's `x`,`y`,`z` are relative to it.
- Cards, tables, lists, text, rects and ellipses are **billboards**: they always face the viewer and scale with distance. The board will not tilt text.
- Built-ins `cube(size,color)` and `pyramid(size,height,color)` are groups of faces; define others the same way with explicit `[x,y,z]` points.
- Camera: `view` accepts `yaw` (degrees, orbit around the vertical axis), `pitch` (-85..85), `center:[x,y,z]`, `zoom`, `fit`, and `mode:"2d"|"3d"`. `state.view` reports `mode`, `yaw`, `pitch`, `cz`. In the UI: right-drag orbits, drag pans, wheel zooms. With yaw = pitch = 0 and every z = 0 the 3D view is identical to 2D.
- Physics pauses while in 3D; `box` values stay 2D world boxes; marks are placed on the z = 0 plane.

```json
[{"op":"create","object":{"id":"tower","type":"cube","x":0,"y":0,"z":0,"size":120,"color":"#7ab8ff"}},
 {"op":"create","object":{"id":"roof","type":"pyramid","x":0,"y":-120,"z":0,"size":120,"height":80,"color":"#ffd479"}},
 {"op":"create","object":{"id":"why","type":"card","x":200,"y":-200,"z":60,"title":"A tower","text":"Two presets stacked in z."}},
 {"op":"view","mode":"3d","center":[0,-60,0],"zoom":1.2,"yaw":30,"pitch":-25,"duration":400}]
```


`select` accepts `"id"`, `["a","b"]`, or a filter: `type` (primitive or preset name), `preset`, `tag`, `parent`, `roots:true`, `ids`, then `fraction`, `slice:[start,end]`, `limit`. `{}` selects everything. Moving a child of a laid-out group is an error (reorder or reparent it instead). Removing a group removes its children and their edges; ungrouping keeps them.

Durations are milliseconds. Create/remove default to 180 ms fades; everything else is immediate unless given `duration`. Numbers and six-digit colours interpolate. Bulk creation makes `dot-0`, `dot-1`, …; `arrange:"grid"` with `spacing` replaces scatter.

## Physics

Roots with `body:true` participate; children move with their root. `pinned` anchors, `mass` resists, an edge with `rest` and `strength` is a spring. Repulsion separates bodies, `center` pulls toward the origin, damping keeps that fraction of velocity per frame, collisions use boxes. Keep body counts modest; work is quadratic.

## Recovery and limits

Loopback service; one human, one visible controlling tab. Scene, presets, 30 undo steps and marks persist in `data/scene.json`; commands append to `data/history.jsonl`. Reconnect loads the current scene without replaying animations. Limits: 2000 objects, 500 per bulk create, 200 commands/request, 10 s per duration, 500 elements per repeat. Clear is undoable; use it only when asked. Preserve the human's ideas.
