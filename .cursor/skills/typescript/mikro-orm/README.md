# MikroORM skills (TypeScript)

Agent skills for **MikroORM v7** under `.cursor/skills/typescript/mikro-orm/`.

Start at [AGENTS.md](AGENTS.md) to pick the right skill.

## Catalog

| Skill | Path | Focus |
|-------|------|--------|
| Fundamentals | `fundamentals/` | EntityManager, Identity Map, Unit of Work, fork, RequestContext |
| Entities | `entities/` | `defineEntity` / `p.*`, relations, embeddables, collections |
| Querying | `querying/` | find/findOne, operators, populate, QueryBuilder, filters |
| Persistence | `persistence/` | persist/flush, transactions, cascades, soft delete, events |
| Migrations | `migrations/` | Migrator, schema generator, CLI, seeders |
| IdClear | `idclear/` | `@idclear/db`, Effect bridges, monorepo layout |

## Related

- Feature conversion WoW: `.cursor/skills/mikroorm-feature-conversion/` (if present)
- Effect layers: `.cursor/skills/typescript/effect.ts-*`
- ADR: `docs/adr/0001-mikroorm-products-pilot.md`

## Sources

Primary documentation researched from [mikro-orm.io](https://mikro-orm.io/docs) (Context7: `/websites/mikro-orm_io`, `/websites/mikro-orm_io_next`). Skills target **v7** as used in this monorepo.
