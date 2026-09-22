# Reusable idea board

## Purpose

Externalize a person's thinking through voice-directed construction of a spatial scene. The first use is designing a TOML schema; the software has no TOML-specific data model. The human supplies the relationships and intent. A model translates them into short, predictable commands.

## Architecture

1. A loopback Python service owns the object registry, validates commands, resolves selectors and writes a recoverable snapshot and command history.
2. A browser receives commands over SSE. Existing SVG object elements keep their identities. A local queue animates changes and preserves the order of operations, even when an entire sequence arrives in one request.
3. A local animation/physics loop advances motion without model calls. Springs, repulsion, center attraction, damping, simple collisions, pinning and impulses are optional. Physics is an expressive aid, not an engineering simulator.
4. A small CLI sends JSON. An operator manual documents the public vocabulary and examples. Future models need only the manual and board state.

## Drawing and behavior

Objects: dot, ellipse, rectangle, diamond, polygon, line, arrow, text, card, code, path, group. Connections attach to object IDs. Groups hold members and can act as a single movable body. User-defined templates combine primitives into reusable props.

Commands: create, set, remove, move, group, ungroup, link, animate, impulse, physics, layout, fit, wait, undo, redo. Selection accepts IDs or filters, then slice/fraction. Bulk creation and local staggering avoid expanding a simple instruction into hundreds of model-generated tokens.

## Acceptance criteria

- One command removes a selected half progressively; the next recolors surviving existing objects after the removal completes.
- One command animates a compound pyramid; its members maintain their relative positions.
- Pinned objects stay fixed, linked bodies exert spring forces, and collisions displace other bodies.
- Commands are rejected before mutation if invalid. Payloads are data only, never executable code.
- New connections receive a current snapshot. Scene state and a finite undo history survive service restart.
- Frame acknowledgments distinguish receipt, first visible frame and completion. Measure server acceptance to frame time separately from model/tool latency.
- A fresh reviewer can operate the board through documented commands without reading source.

## Scope

Local, single-user board, optional mouse controls, dependency-free runtime. SVG keeps text and code readable and avoids introducing a rendering framework solely for these primitives. Approximate 2D collision bounds, no 3D, rigid-body rotation or general constraint solver. Model reasoning, speech recognition and voice endpointing remain outside the board.
