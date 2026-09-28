---
name: voice-whiteboard
description: Materialize and drive a live whiteboard in the user's browser (cards, tables, lists, diagrams, arrows, 3D, physics) by sending JSON commands. Use when the user wants to think visually, sketch a spec or architecture, or says "put it on the board".
user-invocable: true
metadata:
  tags: [whiteboard, diagram, visual, voice, board, canvas]
---

# Voice whiteboard

A self-contained loopback whiteboard: a small Python server (standard library only) and a browser page. The human watches and points; you place, link, edit and arrange objects with JSON. The whole app is in `app/` beside this file.

## Start

```sh
python3 <this skill dir>/app/boardctl.py start --data ~/.local/share/voice-whiteboard
python3 <this skill dir>/app/boardctl.py open      # once, if the user has no tab open
```

`start` attaches to a board already serving http://127.0.0.1:8770 (for example one started by the user's `voice-whiteboard` launcher) and otherwise starts this copy. `--data` only matters when this copy starts the server; the scene, presets, undo history and marks persist there. Use `--port N` on every call to run a separate board.

## Operate

Read `app/QUICKSTART.md`, then `app/DRIVER.md` once. The DRIVER is the full command vocabulary. Every later call is `python3 <this skill dir>/app/boardctl.py <start|send|state|marks|open>`. Send commands with `send '<JSON>' --wait`, read the resolved boxes with `state`, and read what the user pointed at with `marks`. Never edit the HTML or JS to change what's on the board.

Etiquette: preserve what's already there, put new material in free space (compare `state` boxes), frame it with `{"op":"view","fit":[ids]}`, and keep `clear` for when the user asks for it.

Shared board: other sessions may be driving it at the same time. Prefix your ids with a short session tag, re-read `state` before placing things, and use `marks --peek` to look at marks another session is handling without taking them; plain `marks` takes the batch so the user's next clicks start fresh. `undo` and `clear` act on the whole board, including other sessions' work.
