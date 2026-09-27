# Idea board

A local idea sandbox driven by compact JSON commands. The browser draws, animates and simulates shapes, text, code, links and compound props independently of the model. Its first use is TOML schema design, but it has no domain-specific data model.

```sh
python3 boardctl.py start
python3 boardctl.py open
```

Runtime requirements: Python 3 and a modern browser. No package install, build, CDN or cloud service. `open` uses Linux xdg-open. `board_server.py --port N --data PATH` runs in the foreground; `boardctl.py start --data PATH` (or `$VOICE_WHITEBOARD_DATA`) picks the scene directory.

`./install.sh` links the `voice-whiteboard` launcher into `~/.local/bin` and rebuilds the self-contained agent skill (`~/.bnuuy-agents/skills/voice-whiteboard`, linked from `~/.agents/skills`) from this checkout. Rerun it after changing the board. The launcher starts or attaches to the board, opens it, and prints the one line to give an agent (`voice-whiteboard --help`).

- [Fast model prompt](QUICKSTART.md)
- [Command reference](DRIVER.md)
- [Design](PLAN.md)
- [Verification](VERIFICATION.md)

Mouse support is optional: drag objects/canvas, zoom, edit text by double-clicking, undo/redo, pause motion. The command console accepts the same JSON as the CLI. Voice recognition and model invocation remain in the existing conversation.

Tests: `python3 -m unittest -v test_model.py` and `node --test test_engine.js test_camera.js`. Browser tests are served only with `board_server.py --test` at `/test-browser.html`; use isolated data because the harness clears its scene. State is under `data/`; prototype files remain separate.
