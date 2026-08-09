---
name: id-workflow
description: >-
  Industrial Delivery (ID) mode-gated agent pipeline — ORIENT→RESEARCH→PLAN→EXECUTE→REVIEW→SHIP
  with write bans, Maestro tracking, and lane sizing. Use when user invokes /id, /id-*,
  mentions ID workflow, or wants structured guiderails instead of stacking /quality /skills /agent.
---

# ID Workflow Skill

Canonical pack: `.cursor/commands/id-workflow/` (PROTOCOL, lanes, modes, checklists).

## When to use

- User runs `/id` or `/id-<mode>`
- Multi-step work that needs research/plan gates before edits
- Replacing ad-hoc `/skills` `/quality` `/agent` stacks

## Quick protocol

1. Every response: `[ID:<MODE>]` + `lane:<tiny|normal|heavy>`
2. Write ban outside EXECUTE/SHIP (PLAN: Maestro/plans only)
3. Human approve before EXECUTE (unless plan+implement)
4. Tracker = Maestro only
5. PLAN body = `/plan-hierarchically`; do not fork it
6. No Maestro worktrees — claim via CLI `--skip-worktree`; keep edits on the current branch

## Mode files

| Mode | Path |
|------|------|
| ORIENT | `commands/id-workflow/modes/orient.md` |
| RESEARCH | `…/modes/research.md` |
| PLAN | `…/modes/plan.md` |
| EXECUTE | `…/modes/execute.md` |
| REVIEW | `…/modes/review.md` |
| SHIP | `…/modes/ship.md` |

## Agents

`.cursor/rules/agent-{researcher,architect,implementer,reviewer}.mdc`

## Related

- Rule summary: `.cursor/rules/id-workflow.mdc`
- Research cites: `commands/id-workflow/RESEARCH.md`
