# Operating this board

To start a board, draw, manipulate objects or translate a user's spoken instructions, read `QUICKSTART.md`, then the public vocabulary in `DRIVER.md`. Operate through `boardctl.py send` and inspect with `boardctl.py state`. No renderer knowledge is required for board operation.

For implementation changes, `PLAN.md` contains the design contract and `VERIFICATION.md` records checked behavior. Preserve the separate prototype and saved scenes. Exercise both model/engine tests and the isolated browser sequence test when changing animation, selection, persistence or physics.
