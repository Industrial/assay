---
description: Run the strategy portfolio research loop (diagnose → decide → prove → integrate → commit)
---

# Strategy Portfolio Loop

Load and follow the project skill before any portfolio work:

**Skill:** `.cursor/skills/trading/strategy-portfolio-loop/SKILL.md`  
**Reference:** `.cursor/skills/trading/strategy-portfolio-loop/reference.md`

## Immediate actions

1. Read the skill + reference end-to-end.
2. Load the active book: `portfolio/*.toml` (prefer the newest baseline unless the user named one).
3. Run **one** cycle of the operating loop (Diagnose → Decide → Prove → Integrate → Commit).
4. Prefer repo hooks:
   - `devenv shell -- python scripts/portfolio_backtest.py --portfolio portfolio/<name>.toml`
   - `devenv shell -- python scripts/portfolio_aggregate.py …`
5. Open / regenerate `runs/portfolio/<ts>/report.html` and interpret portfolio-level metrics (not sleeve IS heroes).

## Constraints

- Choose **exactly one** Action Menu item per cycle.
- Promote sleeves only through portfolio aggregate acceptance.
- Partitioned-capital model is not shared-margin — say so in every report/decision note.
- On accept: update `portfolio/*.toml` + write `runs/portfolio/<ts>/`. On reject: leave baseline unchanged and log why.

User request / context:

$ARGUMENTS
