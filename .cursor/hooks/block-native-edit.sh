#!/usr/bin/env bash
# Blocks native Write/StrReplace/Delete/Edit; agents must use lean-ctx ctx_edit.
# Evidence: .cursor/rules/lean-ctx.mdc

set -euo pipefail
input=$(cat)
tool_name=$(echo "$input" | jq -r '.tool_name // .toolName // empty')

case "$tool_name" in
    Write|StrReplace|Delete|Edit|ApplyPatch|EditNotebook)
        cat <<'EOF'
{
  "permission": "deny",
  "user_message": "Native file edit tools are disabled. Use lean-ctx ctx_edit via MCP.",
  "agent_message": "STOP. Do not retry this native edit tool. Use CallMcpTool with server project-0-solana-yield-optimizer-lean-ctx (or lean-ctx) and tool ctx_edit: path + old_string + new_string for edits; path + new_string + create:true for new files; replace_all optional. GetMcpTools once for schema, then edit. Never loop on Write/StrReplace/Edit/Delete."
}
EOF
        exit 0
        ;;
esac

echo '{"permission":"allow"}'
exit 0
