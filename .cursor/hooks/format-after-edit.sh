#!/usr/bin/env bash
# Runs treefmt on the repo after Agent file edits.
set -e
input=$(cat)
workspace_root=$(echo "$input" | jq -r '.workspace_roots[0] // ""')

if [[ -n "$workspace_root" && -d "$workspace_root" ]]; then
    (cd "$workspace_root" && devenv shell -- moon run format 2>/dev/null) \
        || (cd "$workspace_root" && devenv shell -- treefmt 2>/dev/null) \
        || true
fi
exit 0
