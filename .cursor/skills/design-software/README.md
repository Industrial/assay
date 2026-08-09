# Software design skills for Cursor

Curated agent skills for Domain-Driven Design, Event Sourcing, CQRS, Hexagonal/Clean Architecture, and related design methodologies under `.cursor/skills/design-software/`.

**15 skills** — strategic DDD, tactical modeling, event-sourced systems, and architecture decision support.

## Quick pick

| Task | Skill |
|------|-------|
| Model a complex business domain (aggregates, VOs, events) | `domain-driven-design` |
| Strategic DDD: subdomains, core/supporting/generic | `ddd-strategic-design` |
| Tactical DDD checklist (entities, aggregates, repos) | `ddd-tactical-patterns` |
| Context maps and integration patterns | `ddd-context-mapping` |
| Shared glossary / ubiquitous language | `ubiquitous-language` |
| Collaborative domain modeling session artifacts | `domain-modeling` |
| Persist state as event streams (Decider, projections) | `event-sourcing` |
| Separate command/query models | `cqrs-implementation` |
| Design an event store and stream layout | `event-store-design` |
| Long-running workflows / compensations | `saga-orchestration` |
| Ports & adapters / dependency inversion | `hexagonal-architecture` |
| Layered Clean Architecture dependency rule | `clean-architecture` |
| Backend architecture pattern survey (Clean/Hex/DDD) | `architecture-patterns` |
| Write Architecture Decision Records | `architecture-decision-records` |
| Broad system design trade-offs | `system-design` |

## Recommended workflow

1. **Discover** — `domain-modeling` + `ubiquitous-language`
2. **Strategize** — `ddd-strategic-design` → `ddd-context-mapping`
3. **Model** — `domain-driven-design` + `ddd-tactical-patterns`
4. **Structure** — `hexagonal-architecture` and/or `clean-architecture`
5. **Scale reads/writes** — `cqrs-implementation`; escalate to `event-sourcing` only when justified
6. **Orchestrate** — `saga-orchestration` for multi-aggregate workflows
7. **Record** — `architecture-decision-records`

## Catalog

### citypaul/.dotfiles ([citypaul/.dotfiles](https://github.com/citypaul/.dotfiles))

Deep, resource-backed skills (MIT).

| Skill | Notes |
|-------|-------|
| `domain-driven-design` | Aggregates, VOs, domain events/services, bounded contexts + resources |
| `event-sourcing` | Decider fold, event store, projections, versioning + resources |
| `hexagonal-architecture` | Ports/adapters, CQRS-lite, incremental adoption + resources |
| `ubiquitous-language` | Glossary format and language protocol |

### Agentic Awesome Skills ([sickn33/agentic-awesome-skills](https://github.com/sickn33/agentic-awesome-skills))

Also mirrored by [contraktor-tech/cursor-skills](https://github.com/contraktor-tech/cursor-skills). MIT + CC BY 4.0 content.

| Skill | Notes |
|-------|-------|
| `ddd-strategic-design` | Subdomain classification templates |
| `ddd-tactical-patterns` | Tactical building-block checklist |
| `ddd-context-mapping` | Context map relationship patterns |
| `domain-modeling` | ADR/context session formats |
| `cqrs-implementation` | Read/write split + playbook |
| `event-store-design` | Stream design playbook |
| `saga-orchestration` | Process managers / compensations |
| `architecture-patterns` | Clean/Hex/DDD pattern guide |
| `architecture-decision-records` | ADR authoring |

### Wondel.ai skills ([wondelai/skills](https://github.com/wondelai/skills))

MIT.

| Skill | Notes |
|-------|-------|
| `clean-architecture` | Dependency rule, layers, ports |
| `system-design` | Capacity, consistency, and trade-off framing |

## Updating

```bash
DEST=".cursor/skills/design-software"
# Re-copy from upstream clones listed above, keep LICENSE-* files and this README.
```
