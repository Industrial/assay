# Gleam / Lustre Cursor skills

Vendored agent skills for idiomatic Gleam and Lustre frontend work. Layout mirrors `skills/elixir/` and `skills/temporal/`.

## Quick pick

| Task | Skill |
|------|-------|
| Pure Gleam language, types, decoding, library design | `gleam` |
| Compact idiomatic Gleam + common libs (OTP, Wisp, Lustre, FFI) | `gleam-idiomatic` |
| Lustre 5.x MVU, forms, routing, HTTP (rsvp/modem) | `lustre` |
| Concise Lustre MVU / events / effects cheat sheet | `lustre-guide` |
| Wisp/Mist REST + JSON APIs | `api-gleam` |
| OTP, Mist, Wisp, auth, SQL (Squirrel/Parrot) | `gleam-backend` |
| Lustre unit / component testing | `frontend-testing` |
| Postgres + Gleam (POG / Cigogne / Squirrel) | `pg-gleam` |
| Gleam TDD workflow | `tdd` |

## Catalog

### gleam

Core Gleam fundamentals from [aboio-labs/aboio-skills](https://github.com/aboio-labs/aboio-skills) — language basics, type design, decoding, error handling, library/FFI. Prefer this for pure Gleam (not HTTP UI).

### gleam-idiomatic

From [stoft/gleam-skill](https://github.com/stoft/gleam-skill). Single-skill workflow with on-demand `references/` (stdlib, OTP, Wisp, Lustre, `gleam_javascript`, plinth). Good default when the agent needs a broad Gleam primer.

### lustre

Frontend skill targeting Lustre 5.x — MVU, advanced forms, components, SPA routing (`modem`), HTTP (`rsvp`), browser APIs. From aboio-skills.

### lustre-guide

Compact Lustre MVU guide adapted from [Scouterna/j26-booking](https://github.com/Scouterna/j26-booking) (`.claude/skills/lustre-guide`). Complements `lustre` when you want a shorter architecture reference.

### api-gleam

REST/JSON API patterns for Gleam + Wisp + Mist (routes, errors, pagination, middleware, SPA routing).

### gleam-backend

Backend stack: OTP, Mist, Wisp, auth/JWT, Squirrel/Parrot, Cigogne, logging.

### frontend-testing

Lustre testing patterns.

### pg-gleam

Postgres performance and schema/query patterns for Gleam + Squirrel/Parrot + POG + Cigogne.

### tdd

Gleam-oriented TDD skill from aboio-skills.

## Sources & licenses

| Source | Skills | License |
|--------|--------|---------|
| [aboio-labs/aboio-skills](https://github.com/aboio-labs/aboio-skills) | `gleam`, `lustre`, `api-gleam`, `gleam-backend`, `frontend-testing`, `pg-gleam`, `tdd` | MIT (`LICENSE-aboio-skills`) |
| [stoft/gleam-skill](https://github.com/stoft/gleam-skill) | `gleam-idiomatic` | MIT (in skill dir) |
| [Scouterna/j26-booking](https://github.com/Scouterna/j26-booking) `.claude/skills/lustre-guide` | `lustre-guide` | MIT |

Upstream content is experimental / community-maintained — not official Gleam or Lustre docs. Prefer Context7 / hexdocs for API truth when versions diverge.
