# Graphic / UI design skills for Cursor

Curated, high-install design and UI/UX agent skills installed under `.cursor/skills/design-graphic/`. Sourced from the most-used upstream collections (May 2026): Anthropic official skills, Impeccable, ibelick/ui-skills, and spencerpauly/awesome-cursor-skills.

**15 skills** — generation, tokens, brand, visual QA, accessibility, and motion.

## Quick pick

| Task | Skill |
|------|-------|
| Build a page, dashboard, or component with strong aesthetics | `frontend-design` |
| Refine, audit, or iterate UI (brand vs product modes) | `impeccable` |
| Enforce Tailwind UI baseline (spacing, typography, states) | `baseline-ui` |
| Fix WCAG / ARIA / keyboard / contrast issues | `fixing-accessibility` |
| Fix animation jank and motion performance | `fixing-motion-performance` |
| Create reusable color/type/spacing themes | `theme-factory` |
| Apply consistent brand colors and typography | `brand-guidelines` |
| Generate poster/visual art (PNG/PDF) | `canvas-design` |
| Build self-contained HTML/React artifacts | `web-artifacts-builder` |
| Enforce 8px grid, tokens, dark mode on new components | `using-ui-stack` |
| Screenshot + console/network QA after UI changes | `visual-qa-testing` |
| Test mobile / tablet / desktop layouts | `responsive-testing` |
| Verify light/dark token mappings | `dark-mode-testing` |
| ARIA tree accessibility audit in browser | `accessibility-auditing` |
| Visual before/after across git branches | `comparing-branches-visually` |

## Recommended workflow

1. **Generate** — `frontend-design` or `impeccable` (`craft` / `shape`)
2. **Systematize** — `theme-factory` + `using-ui-stack` or project tokens
3. **Harden** — `baseline-ui` → `fixing-accessibility` → `fixing-motion-performance`
4. **Verify** — `visual-qa-testing`, `responsive-testing`, `dark-mode-testing`, `accessibility-auditing`

Pair with existing repo skills: `react/web-design-guidelines` (100+ UX/a11y rules), `nextjs/shadcn-skills`, and Playwright skills under `.cursor/skills/playwright/`.

## Catalog

### Anthropic official ([anthropics/skills](https://github.com/anthropics/skills))

| Skill | Notes |
|-------|-------|
| `frontend-design` | Canonical anti–AI-slop UI skill (~277k+ installs on skills.sh) |
| `theme-factory` | 10 preset themes + token generation |
| `brand-guidelines` | Brand colors and typography for artifacts |
| `canvas-design` | Visual design in PNG/PDF (fonts + assets included) |
| `web-artifacts-builder` | Bundled HTML/React artifact scaffolding |

License: see each skill's `LICENSE.txt` where present (Apache 2.0 for open skills).

### Impeccable ([pbakaus/impeccable](https://github.com/pbakaus/impeccable))

| Skill | Notes |
|-------|-------|
| `impeccable` | Brand vs product registers, 23+ commands, anti-pattern detector, live browser iteration |

License: Apache 2.0 (`impeccable/LICENSE`). Paths patched for `.cursor/skills/design-graphic/impeccable/`.

### UI Skills ([ibelick/ui-skills](https://github.com/ibelick/ui-skills))

| Skill | Notes |
|-------|-------|
| `baseline-ui` | Tailwind UI constraints for design engineers |
| `fixing-accessibility` | WCAG-focused HTML/React fixes |
| `fixing-motion-performance` | Motion and animation performance |

### Cursor-native QA ([spencerpauly/awesome-cursor-skills](https://github.com/spencerpauly/awesome-cursor-skills))

| Skill | Notes |
|-------|-------|
| `using-ui-stack` | Design system enforcement on AI-generated components |
| `visual-qa-testing` | Built-in browser screenshots + console/network |
| `responsive-testing` | Multi-viewport layout checks |
| `dark-mode-testing` | Light/dark token regression |
| `accessibility-auditing` | Browser aria-tree audit |
| `comparing-branches-visually` | Side-by-side branch screenshots for PRs |

## Updating

```bash
DEST=".cursor/skills/design-graphic"
TMP=$(mktemp -d)

# Anthropic design skills
git clone --depth 1 --filter=blob:none --sparse https://github.com/anthropics/skills.git "$TMP/skills"
cd "$TMP/skills"
git sparse-checkout set skills/frontend-design skills/theme-factory skills/brand-guidelines skills/canvas-design skills/web-artifacts-builder
for s in frontend-design theme-factory brand-guidelines canvas-design web-artifacts-builder; do
  rm -rf "$DEST/$s" && cp -a "skills/$s" "$DEST/$s"
done

# Impeccable
git clone --depth 1 --filter=blob:none --sparse https://github.com/pbakaus/impeccable.git "$TMP/impeccable"
cd "$TMP/impeccable" && git sparse-checkout set .cursor/skills/impeccable
rm -rf "$DEST/impeccable" && cp -a .cursor/skills/impeccable "$DEST/impeccable"
find "$DEST/impeccable" -type f \( -name "*.md" -o -name "*.mjs" -o -name "*.json" \) -print0 \
  | xargs -0 sed -i 's|.cursor/skills/impeccable|.cursor/skills/design-graphic/impeccable|g'

# ibelick/ui-skills
git clone --depth 1 --filter=blob:none --sparse https://github.com/ibelick/ui-skills.git "$TMP/ui-skills"
cd "$TMP/ui-skills" && git sparse-checkout set skills/baseline-ui skills/fixing-accessibility skills/fixing-motion-performance
for s in baseline-ui fixing-accessibility fixing-motion-performance; do
  rm -rf "$DEST/$s" && cp -a "skills/$s" "$DEST/$s"
done

# awesome-cursor-skills (design QA)
git clone --depth 1 --filter=blob:none --sparse https://github.com/spencerpauly/awesome-cursor-skills.git "$TMP/acs"
cd "$TMP/acs"
for s in using-ui-stack responsive-testing dark-mode-testing accessibility-auditing visual-qa-testing comparing-branches-visually; do
  git sparse-checkout add "resources/$s"
done
for s in using-ui-stack responsive-testing dark-mode-testing accessibility-auditing visual-qa-testing comparing-branches-visually; do
  rm -rf "$DEST/$s" && cp -a "resources/$s" "$DEST/$s"
done
```

Run inside `devenv shell --` when this repo requires it.
