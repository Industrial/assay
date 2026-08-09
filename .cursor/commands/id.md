# /id — Industrial Delivery orchestrator

Load and obey `.cursor/commands/id-workflow/PROTOCOL.md`.

1. Start in **ORIENT**: follow `id-workflow/modes/orient.md` and `checklists/orient-exit.md`.
2. Declare every response: `[ID:<MODE>]` then `lane:<tiny|normal|heavy>`.
3. Auto-route by lane (`id-workflow/lanes.md`):
   - `tiny` + clear → brief RESEARCH or EXECUTE
   - `normal`/`heavy` → RESEARCH → PLAN (run `/plan-hierarchically` body) → wait for approve → EXECUTE → REVIEW → SHIP
4. Mode playbooks: `id-workflow/modes/<mode>.md`. Do not invent a second tracker — Maestro only.
5. Prefer this over stacking `/quality` `/skills` `/agent` alone; still run those steps inside ORIENT.
