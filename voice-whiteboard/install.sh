#!/usr/bin/env bash
# Install the launcher into ~/.local/bin and (re)build the self-contained skill from this checkout.
# Rerun after changing the board so the skill's copy stays current.
set -euo pipefail

repo=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
store=$HOME/.bnuuy-agents/skills/voice-whiteboard
link=$HOME/.agents/skills/voice-whiteboard
runtime=(board_server.py model.py boardctl.py board.html board.js board.css engine.js camera.js presets.json DRIVER.md QUICKSTART.md)

mkdir -p "$HOME/.local/bin"
ln -sfn "$repo/bin/voice-whiteboard" "$HOME/.local/bin/voice-whiteboard"

rm -rf "$store.tmp"
mkdir -p "$store.tmp/app"
cp "$repo/skill/SKILL.md" "$store.tmp/"
for f in "${runtime[@]}"; do cp "$repo/$f" "$store.tmp/app/"; done
echo "built from $(git -C "$repo" rev-parse --short HEAD)$(git -C "$repo" diff --quiet -- "${runtime[@]}" skill || echo '-dirty')" >"$store.tmp/app/VERSION"
rm -rf "$store"
mv "$store.tmp" "$store"

mkdir -p "$(dirname "$link")"
if [ -L "$link" ]; then
    ln -rsfn "$store" "$link"
else
    if [ -e "$link" ]; then
        backup="$link.bak-$(date +%s)"
        mv -- "$link" "$backup"
        echo "moved existing skill to $backup"
    fi
    ln -rs "$store" "$link"
fi

echo "launcher: $HOME/.local/bin/voice-whiteboard -> $repo/bin/voice-whiteboard"
echo "skill:    $link -> $(readlink "$link") ($(cat "$store/app/VERSION"))"
