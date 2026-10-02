# Legacy prototype commands

For the CURRENT reusable board on port 8770, read [DRIVER.md](DRIVER.md).
The shortest driver prompt is [QUICKSTART.md](QUICKSTART.md).
Everything below describes the preserved port-8769 prototype, not the current interface.

Open http://127.0.0.1:8769. Server: python3 /home/shayna/voice-whiteboard/server.py

POST JSON objects or an array to http://127.0.0.1:8769/commands.
The browser receives each command over Server-Sent Events and changes the corresponding object without navigating or reloading.

Operations: create, update, remove, clear, fit.
Object types: dot, ellipse, rectangle, text, card, code, line, arrow, edge, group.
Fields: id, type, x, y, width, height, color, fill, title, text.
Edges use from/to object IDs, or line/arrow use x/y and x2/y2 relative to x/y.
Groups use members, an array of object IDs, with text and color.

Examples:
```json
[
  {"op":"create","id":"a","type":"card","x":-300,"y":0,"title":"Definition","text":"A value has a type."},
  {"op":"create","id":"b","type":"code","x":100,"y":0,"title":"TOML","text":"count = 3"},
  {"op":"create","id":"ab","type":"arrow","from":"a","to":"b","text":"represented by"},
  {"op":"update","id":"a","color":"red"},
  {"op":"create","id":"g","type":"group","members":["a","b"],"color":"red","text":"Reconcile these"},
  {"op":"fit"}
]
```

GET /status reports connected streams, event count and last browser acknowledgment.
events.jsonl persists commands for reconnect and restart. Old index.html and its state remain separate.
Use JSON commands for future board edits. Do not edit HTML or inject scripts to change content.
