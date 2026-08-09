#!/usr/bin/env bash
# Blocks native SemanticSearch; agents must use roam-code (or lean-ctx) MCP instead.
# Evidence: roam-code INSTRUCTIONS.md, lean-ctx INSTRUCTIONS.md

set -euo pipefail
input=$(cat)
tool_name=$(echo "$input" | jq -r '.tool_name // .toolName // empty')

case "$tool_name" in
    SemanticSearch)
        cat <<'EOF'
{
  "permission": "deny",
  "user_message": "Native SemanticSearch is disabled. Use roam-code or lean-ctx MCP for code exploration.",
  "agent_message": "STOP. Do not retry native SemanticSearch. Prefer CallMcpTool with server project-0-solana-yield-optimizer-roam-code (or roam-code): roam_explore, roam_understand, roam_search_symbol, roam_context, roam_trace, roam_uses. For token-compressed semantic search use lean-ctx ctx_semantic_search. GetMcpTools once for schema, then explore."
}
EOF
        exit 0
        ;;
esac

echo '{"permission":"allow"}'
exit 0
