---
name: voice-whiteboard
description: Materialize and drive a live whiteboard in the user's browser (cards, tables, lists, diagrams, arrows, 3D, physics) with JSON commands. Use when the user wants to think visually, sketch a spec or architecture, or says "put it on the board".
user-invocable: true
metadata:
  tags: [whiteboard, diagram, visual, voice, board, canvas]
---

# Voice whiteboard

The board ships with this repository: `voice-whiteboard/` at the repository root (three levels above this file). It is a loopback server (Python 3 standard library) and a browser page. The user talks and points; you place, link, edit and arrange objects.

## Start

```sh
python3 voice-whiteboard/boardctl.py start      # from the repository root
python3 voice-whiteboard/boardctl.py open       # once, if the user has no tab open
```

`start` attaches to any board already serving http://127.0.0.1:8770, whoever started it, and otherwise starts one whose scene persists in `voice-whiteboard/data/`. Pass `--port N` on every call for a separate board, and `--data DIR` on `start` to keep the scene elsewhere.

## Operate

Read `voice-whiteboard/QUICKSTART.md`, then `voice-whiteboard/DRIVER.md` once; the DRIVER is the full command vocabulary. After that every action is `boardctl.py send '<JSON>' --wait`, with `state` for the resolved boxes. When the user says "this", "here" or a number you didn't create, run `boardctl.py marks`: it returns the numbered spots they clicked and marks that batch handled, so their next clicks start a fresh batch.

Shared board: other sessions may be driving it at the same time. Prefix your ids with a short session tag, re-read `state` before placing things in free space, and use `marks --peek` to look at marks another session is handling without taking them. `undo` and `clear` act on the whole board, including other sessions' work, so use them only when the user asks.
