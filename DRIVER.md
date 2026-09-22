# Drive the idea board

The human supplies meaning. Translate their request into the smallest suitable commands and send promptly. The browser owns animation and physics. You need this contract and object IDs, not the implementation.

## Start and inspect

```sh
python3 /home/shayna/voice-whiteboard/boardctl.py start
python3 /home/shayna/voice-whiteboard/boardctl.py open
python3 /home/shayna/voice-whiteboard/boardctl.py state
```

The page is http://127.0.0.1:8770. Start is idempotent; open once. `state` returns objects indexed by ID, templates, physics settings, sequence number, the human's current `view` and any unread `marks`. Coordinates are world pixels: x rightward, y downward. Shape x/y is the top-left; polygon/path points are relative offsets. Pan/zoom changes only the view.

## Know what the human sees

`state.view` is the visible tab's camera: `{"cx","cy","zoom","w","h"}` — world centre, zoom and the world-space width/height on screen. Anything outside `cx±w/2`, `cy±h/2` is off-screen for them. Move their camera with `view` (below) rather than guessing.

Every card/code/text object carries a browser-reported `measured` field: `{"w","h","contentW","contentH","overflow"}`. Cards with no `height` grow to fit their text, so `overflow` is false. A card with an explicit `height` that is too small reports `overflow:true` with the height the content actually needs; the board also draws a red dashed outline. Fix it with `{"op":"set","select":"id","props":{"height":null}}` (null deletes a field, returning the card to auto-height) or a larger `width`. You never need a screenshot to know whether text fits.

## Read the human's marks

The human can point. In Point mode (button, `M`, or Alt-click) each click leaves a numbered mark — 1, 2, 3 — optionally with a typed note, on an object or on empty space. Marks are for you; they are not scene objects.

```sh
python3 /home/shayna/voice-whiteboard/boardctl.py marks          # returns unread marks and consumes them
python3 /home/shayna/voice-whiteboard/boardctl.py marks --peek   # look without consuming
```

Each mark: `{"n":1,"batch":3,"x":…,"y":…,"target":"card-id"|null,"note":"…"}`. `target` is the object under the click (its root group if grouped). When the human says "link 1 and 2" or "move these to 3", read the marks and resolve the numbers. Consumed marks stay visible but dimmed until newer batches push them out, and numbering restarts at 1 for the next batch. Check `marks` whenever an instruction refers to "this", "these", "here" or a number you did not create.

## Send and confirm

```sh
python3 /home/shayna/voice-whiteboard/boardctl.py send '{"op":"create","object":{"id":"idea","type":"card","x":-200,"y":0,"title":"An idea","text":"Its definition."}}' --wait
```

JSON can also arrive on stdin. HTTP equivalent: POST `/commands` with Content-Type `application/json`. Send one command, an array, or `{"requestId":"unique-id","commands":[...]}` to deduplicate retries during the current server run. A complete sequence is validated before acceptance, then executes in order. Send early steps before continuing to reason. Content changes require no HTML edits, code injection, reloads or navigation.

`--wait` waits up to 10 seconds for a browser completion. HTTP success alone means accepted. `status` reports per-browser first-frame and completion milliseconds from server acceptance. These exclude model inference/tool dispatch. Prefer a current visible client's receipt; background tabs can be throttled. Frame callbacks indicate a rendering opportunity, not proof the human saw a particular pixel.

## Selection

`select` accepts `"id"`, `["a","b"]`, or filters such as `{"type":"dot","tag":"test","fraction":0.5}`.

Filters combine with AND: `ids`, `type`, `tag`, `roots:true`, then `fraction`, `slice:[start,end]`, `limit`. Fraction selects first floor(N × fraction), in creation order. Slice end is exclusive; negative indices work. Roots excludes members owned by groups. `{}` selects all. A missing explicit ID is an error; zero filter matches is a safe no-op.

## Commands

| Intent | Example |
|---|---|
| Create | `{"op":"create","object":{"id":"a","type":"dot","x":0,"y":0}}` |
| Create many | `{"op":"create","object":{"id":"dot","type":"dot","tags":["test"]},"count":100,"spread":350,"seed":4,"stagger":8}` |
| Edit | `{"op":"set","select":"a","props":{"color":"#ff596b","text":"Updated"}}` |
| Remove progressively | `{"op":"remove","select":{"type":"dot","fraction":0.5},"duration":150,"stagger":15}` |
| Move by offset | `{"op":"move","select":"a","by":[200,0],"duration":600}` |
| Move one to position | `{"op":"move","select":"a","to":[0,0],"duration":400}` |
| Connect | `{"op":"link","id":"ab","from":"a","to":"b","arrow":true,"props":{"text":"requires"}}` |
| Compound / weld | `{"op":"group","id":"assembly","select":["a","b"],"props":{"outline":false}}` |
| Box related objects | `{"op":"group","id":"question","select":["a","b"],"props":{"color":"#ff596b","title":"Reconcile these"}}` |
| Release members | `{"op":"ungroup","select":"assembly"}` |
| Pin | `{"op":"set","select":"a","props":{"pinned":true}}` |
| Throw | `{"op":"impulse","select":"assembly","velocity":[700,0]}` |
| Pause physics | `{"op":"physics","props":{"enabled":false}}` |
| Arrange | `{"op":"layout","select":{"roots":true},"mode":"grid","spacing":300,"duration":500}` |
| Fit everything | `{"op":"view","fit":true,"duration":300}` |
| Fit some objects | `{"op":"view","fit":["a","b"]}` or `{"op":"view","fit":{"tag":"toml"}}` |
| Look at a point | `{"op":"view","center":[400,-120],"zoom":1.2,"duration":400}` |
| Pan | `{"op":"view","by":[300,0]}` |
| Wait in sequence | `{"op":"wait","duration":200}` |
| Undo / redo | `{"op":"undo"}` / `{"op":"redo"}` |

Bulk creation makes `dot-0`, `dot-1`, etc. `arrange:"grid"` and `spacing` substitute for scatter. `items:[object,...]` creates different objects at once. Layout also accepts `mode:"scatter"`, `spread`, `seed`; a new seed gives a new arrangement.

Duration and stagger are milliseconds. Create/remove default to 180 ms fades; other commands are immediate by default. Stagger start offsets span at most one second. `animate` aliases `set`; specify duration. Numbers and six-digit hex colors interpolate. Text/booleans change at the end. Position changes use `move`. Set cannot change identity, type or group membership.

Original two-step test, compactly:
```json
[
  {"op":"remove","select":{"type":"dot","fraction":0.5},"duration":150,"stagger":15},
  {"op":"set","select":{"type":"dot"},"props":{"color":"#ff596b"},"duration":180,"stagger":8}
]
```

## Drawing vocabulary

Types: `dot`, `ellipse`, `rectangle`, `diamond`, `polygon`, `line`, `arrow`, `path`, `text`, `card`, `code`, `group`.

Common fields: `id`, `type`, `x`, `y`, `width`, `height`, `color`, `fill`, `opacity`, `strokeWidth`, `tags`. Dot supports `radius`. Text/card/code supports `title`, `text`, `fontSize`. Width defaults to 260 and text wraps inside it; omit `height` and the card grows to fit. Give `height` only when you want a fixed box, and read `measured.overflow` afterwards. Text is literal; code preserves whitespace. This version has no image embeds, Markdown rendering or math typesetting.

Polygon/path use `points:[[x,y],...]`; path also accepts `closed:true`. Lines/arrows use two or more points or paired `from`/`to` IDs. Links attach to shapes/groups, not other links, start and end on the shape boundary and follow their endpoints, with optional `text` labels. Group `members` are IDs; move a group as one object. Each member has one parent. Removing a group removes its members; ungrouping preserves them. Deleting an endpoint removes its links.

Connections and point-lines take `route`: `straight` (default), `curve` (quadratic bend, `curve` sets the bend in pixels, negative bends the other way) or `elbow` (axis-aligned with one bend). Ends take `head` (at `to` / last point) and `tail` (at `from` / first point) from `none`, `arrow` (filled), `open` (chevron), `dot`, `diamond`, `bar`. `arrow:true` on `link` is shorthand for `head:"arrow"`. Examples: a dependency `{"head":"arrow"}`, an association `{"head":"none"}`, a bidirectional flow `{"head":"arrow","tail":"arrow"}`, composition `{"tail":"diamond"}`, an inhibitor `{"head":"bar"}`.

## Behavior vocabulary

Set `body:true` to participate in physics. A root group can be a body; members maintain fixed offsets. `pinned:true` anchors it. `mass` controls resistance. Impulse adds pixels/second and enables physics.

```json
[
  {"op":"set","select":{"type":"card","roots":true},"props":{"body":true}},
  {"op":"link","id":"spring","from":"a","to":"b","props":{"rest":220,"strength":2}},
  {"op":"physics","props":{"enabled":true,"repulsion":1200,"center":0.02,"damping":0.94,"collision":true,"bounce":0.45}}
]
```

`rest` is spring distance; `strength` stiffness. Repulsion separates bodies; center attracts them toward origin. Damping 0..1 preserves that velocity fraction per 60 Hz frame. Bounce 0..1 is restitution. Collisions use axis-aligned bounding boxes. There is no rotation, accurate polygon contact, hinge/friction solver or gravity field. Welding uses groups; anchoring uses pins; flexible relationships use springs. Physics continues locally after the command finishes.

## Reusable props

Define once, spawn repeatedly. Template x/y is relative to spawn position. Local IDs become `instance/local-id`; internal links/groups are remapped. The instance is itself a group.

```json
[
  {"op":"define","name":"pyramid","items":[
    {"id":"outline","type":"path","points":[[0,-100],[-90,55],[0,95],[90,55],[0,-100]],"color":"#ffd479","strokeWidth":3},
    {"id":"ridge","type":"line","points":[[0,-100],[0,95]],"color":"#ffd479","strokeWidth":3},
    {"id":"back","type":"path","points":[[-90,55],[0,20],[90,55]],"color":"#b58c49"},
    {"id":"hidden","type":"line","points":[[0,-100],[0,20]],"color":"#b58c49"}
  ]},
  {"op":"spawn","template":"pyramid","id":"pyramid-1","x":-400,"y":0,"body":true},
  {"op":"move","select":"pyramid-1","by":[250,0],"duration":500}
]
```

## Recovery and limits

Loopback service; one user, one visible controlling tab. Multiple tabs simulate independently and may disagree about physics. Scene/templates and 30 undo states persist in `data/scene.json`; commands append to `data/history.jsonl`. Reconnect loads the current scene rather than replaying animations. Visible clients checkpoint positions every 1.5 seconds while the queue is idle.

Undo covers scene operations, not each physics frame or camera movement. Clear is undoable but use it only when the human requests an empty board. Preserve their ideas during demonstrations. Limits: 1000 objects, 500 per bulk create, 200 commands/request, 10 seconds/duration. Physics work grows quadratically with participating bodies, so keep body counts modest.
