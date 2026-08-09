# MikroORM Skills — Agent Router

MikroORM v7 TypeScript ORM skills for Cursor agents. Prefer official docs via Context7 (`/websites/mikro-orm_io` or `/websites/mikro-orm_io_next`) when APIs look unfamiliar.

**Rule:** Pick **one primary** skill per task. Chain only when needed (e.g. `fundamentals` → `entities` → `idclear`).

## Quick Selection Guide

| Situation | Start here |
|-----------|------------|
| EM / Identity Map / Unit of Work / fork / RequestContext | **fundamentals** |
| `defineEntity`, relations, embeddables, property helpers | **entities** |
| `find` / QueryBuilder / populate / filters / operators | **querying** |
| persist, flush, transactions, cascades, soft delete | **persistence** |
| Migrator, schema diff, CLI, seeders | **migrations** |
| IdClear `@idclear/db`, Effect helpers, models layout | **idclear** |
| Migrating a feature off Drizzle/CRUD onto EM | `mikroorm-feature-conversion` (sibling skill) |

## Skill Files

| Dir | Name | When |
|-----|------|------|
| `fundamentals/` | `mikro-orm-fundamentals` | Core architecture before writing EM code |
| `entities/` | `mikro-orm-entities` | Defining or changing entity schemas |
| `querying/` | `mikro-orm-querying` | Reads, filters, joins, pagination |
| `persistence/` | `mikro-orm-persistence` | Writes, UoW flush, transactions |
| `migrations/` | `mikro-orm-migrations` | DDL / migration generation |
| `idclear/` | `mikro-orm-idclear` | This monorepo’s Effect + platform conventions |

## Common Pipelines

```
New entity:     entities → migrations → idclear
New query path: querying → idclear
Write path:     persistence → idclear
Feature migrate: mikroorm-feature-conversion (uses entities + persistence + idclear)
Debug stale/race: fundamentals (Identity Map / fork)
```

## Repo anchors (IdClear)

- Models: `apps/test-nextjs/src/models`
- Config: `apps/test-nextjs/src/infrastructure/database/mikro-orm.config.ts`
- Platform: `@idclear/db` (`libs/db`) — tags, `emFind`/`emFlush`, soft-delete subscriber
- Packages: `@mikro-orm/core` / `postgresql` / `migrations` `^7.0.0`

## Docs

- Stable: https://mikro-orm.io/docs
- Next (v7): https://mikro-orm.io/docs/next/
