#!/usr/bin/env bash
# Enforces workspace rule: wrap ALL terminal commands with `devenv shell --`.
# Evidence: .cursor/rules/shell.mdc (alwaysApply); Cursor docs "Gate risky operations"
# (https://cursor.com/docs/agent/hooks); egghead "Enforce Cursor Tooling Standards
# with beforeShellExecution". Exit 0 + permission allow, or permission deny with message.

set -e
input=$(cat)
command=$(echo "$input" | jq -r '.command // empty')
cwd=$(echo "$input" | jq -r '.cwd // ""')

# Allow if command is empty or already wrapped
if [[ -z "$command" ]]; then
    echo '{"permission":"allow"}'
    exit 0
fi
trimmed=$(echo "$command" | sed 's/^[[:space:]]*//')
if [[ "$trimmed" == devenv\ shell\ --* ]] || [[ "$trimmed" == "devenv shell --"* ]]; then
    echo '{"permission":"allow"}'
    exit 0
fi

# Deny and instruct agent to use devenv shell
echo "{\"permission\":\"deny\",\"user_message\":\"This workspace requires all terminal commands to be wrapped with \\\"devenv shell --\\\". See .cursor/rules/shell.mdc.\",\"agent_message\":\"Wrap the command with \\\"devenv shell --\\\". Example: devenv shell -- $trimmed\"}"
exit 0
