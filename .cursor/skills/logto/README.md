# Logto skills for Cursor

Curated Logto agent skills under `.cursor/skills/logto/`. Cursor discovers nested folders with `SKILL.md` the same way as other skill groups in this repo.

**4 skills** — public ecosystem is thin compared to Docker/Playwright; this folder combines upstream + official-doc distillations + idclear monorepo conventions.

## Quick pick

| Task | Skill |
|------|-------|
| What is Logto / OSS install | `logto-open-source-auth-infrastructure` |
| Next.js App Router + `@logto/next` | `logto-nextjs-app-router` |
| Management API, orgs, connectors, M2M | `logto-management-api` |
| **This repo** — libs/logto, test-nextjs, e2e | `idclear-logto-monorepo` |

## Important: auth stack in this repo

idclear uses **Logto**, not Clerk or Auth.js. Ignore or deprioritize `.cursor/skills/nextjs/clerk-nextjs-skills` and `authjs-skills` for authentication work here.

## Catalog

### Upstream (Agent Skill Exchange)

- **`logto-open-source-auth-infrastructure`** — [agentskillexchange/skills](https://github.com/agentskillexchange/skills/tree/main/skills/logto-open-source-auth-infrastructure) — high-level overview, Docker/npm install pointers

### Official documentation (distilled skills)

- **`logto-nextjs-app-router`** — From [Logto Next.js App Router quick start](https://docs.logto.io/quick-starts/next-app-router)
- **`logto-management-api`** — From [Logto OpenAPI](https://openapi.logto.io) (Management API groups, M2M auth, org RBAC)

### Project-specific

- **`idclear-logto-monorepo`** — Monorepo paths, scopes, proxy, bootstrap, E2E patterns (maintained in-repo)

## Related in this repo

- **MCP:** Logto OpenAPI MCP at `https://openapi.logto.io/mcp` (optional in `.cursor/mcp.json`)
- **Code:** `libs/logto/`, `libs/authentication/`, `apps/test-nextjs/`, `apps/logto-init/`, `apps/e2e-test/pages/LogtoRegisterPage.ts`
- **History:** `history/logto-shared-origin-local.md`

## Sources & licenses

| Source | Skills | License |
|--------|--------|---------|
| [agentskillexchange/skills](https://github.com/agentskillexchange/skills) | 1 | See upstream |
| [Logto docs / OpenAPI](https://docs.logto.io) | 2 (distilled) | Logto docs terms |
| idclear monorepo | 1 | In-repo |

## Updating

```bash
TMP=$(mktemp -d)
git clone --depth 1 https://github.com/agentskillexchange/skills.git "$TMP/skills"
cp -a "$TMP/skills/skills/logto-open-source-auth-infrastructure" ".cursor/skills/logto/"
# Re-edit logto-nextjs-app-router, logto-management-api, idclear-logto-monorepo manually after doc changes
```
