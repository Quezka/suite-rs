# Quezka Suite design system

- `tokens.json` — the single source: colours (light, dark, high contrast light/dark), type scale, spacing, radii,
  shadow, glass opacities. `../tools/gen_tokens.py` writes it into every codebase. Edit values here only.
- `system/` — the browsable design system (README with the UX rules, controls with previews, cover, fonts), the same
  files as the published page https://claude.ai/artifact/RRbY85bLxYJ561k64zK49x.
- `figma-tokens.json` — the same tokens for Figma: import with the Tokens Studio plugin (sets: global + one per theme),
  then export to Figma Variables if wanted.
