# Mzizi agent skills — published from `nyuchi/mzizi-tools`

**The skills are not in this repo any more.** They live in
[`nyuchi/mzizi-tools`](https://github.com/nyuchi/mzizi-tools) under `mzizi-skills/skills/` and
ship as the public npm package **`@nyuchi/mzizi-skills`**. Git is the source of truth for skill
content; this directory holds no skill files.

## Install them here, or in any consumer repo

```bash
npx skills add @nyuchi/mzizi-skills
```

Or install the whole toolchain as a Claude Code plugin — the skills, the `fundi` agent, the
registry MCP, and the `/mzizi:*` commands in one step:

```
/plugin marketplace add nyuchi/mzizi-tools
/plugin install mzizi@mzizi-tools
```

## What's in the bundle

Design and brand: `bundu-design`, `nyuchi-design`, `mukoko-design`.
Engineering doctrine: `simplify`, `discoverability`, `scaffold-component`,
`ecosystem-app-setup`, `cloudflare-worker-rust`, `mcp-server-cloudflare`.

Read the live list any time — it is served from this portal:

```bash
curl -s https://mzizi.dev/api/v1/skills | jq '.skills[].name'
```

or over the MCP: `list_skills` / `get_skill`.

## Why they moved

Three skills used to be committed here as `.md` files and kept in step with the Supabase
`skills` table by a `scripts/sync-skills.ts` that pulled DB → disk. That arrangement broke down:

- `nyuchi-design-system.md` was superseded by the bundle's `nyuchi-design`, and still described
  the retired L1–L10 "3D architecture" model rather than the DNA helix.
- `ecosystem-app-setup.md` and `scaffold-component.md` were duplicates of registry rows, and
  both drifted — pointing at `@nyuchi/design-cli` and `nyuchi/design-agent-skills`, neither of
  which exists under those names.
- `scripts/sync-skills.ts` wrote to `packages/design-agent-skills/`, a directory removed when
  that package moved out of this repo, so `pnpm skills:sync` could only crash.
- Worst of all it pulled **DB → disk** while `mzizi-tools`' own `sync-skills.mjs` pushes
  **disk → DB**. Two writers, opposite directions, one table: whichever ran last won.

The loop is now cut. `mzizi-tools` owns skill content in git and projects it into the `skills`
collection with `pnpm skills:sync`; this repo only ever **reads** that collection, through
`/api/v1/skills*` and the MCP.

## Editing a skill

Open a PR against `nyuchi/mzizi-tools`, editing `mzizi-skills/skills/<name>/SKILL.md`. Never
edit a copy — not here, not in a consumer repo, and not the DB row directly. Merging to `main`
publishes the package automatically.

## Author-time skills that belong in this repo

If a skill is genuinely specific to working **on the portal codebase** (as opposed to building
with the design system), it can live here as `<name>.md` with `name` + `description` YAML
frontmatter. There are none today. Anything reusable by a consumer belongs in the bundle.
