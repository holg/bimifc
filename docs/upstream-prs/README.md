# Upstream proposals for `louistrue/ifc-lite`

bimifc is a separate Rust-only IFC project, MPL-2.0 like ifc-lite. The
two share no git history and have entirely different parser, rendering,
and UI stacks. That's intentional — they're not converging.

Upstream contributions are scoped to the **parser layer only** —
specifically `rust/core` and `rust/wasm-bindings`. Anything we add to
their codebase has to fit their existing idioms (`EntityScanner` +
`EntityDecoder`, generated `IfcType` enum, `FxHashMap` index pattern
used by `build_geometry_style_index`). We don't ask them to change
their architecture, and we don't import our renderer or UI concepts.

## Drafts

| File | Topic | Status |
|------|-------|--------|
| [01-photometric-extraction.md](01-photometric-extraction.md) | Parse `IfcLightSourceGoniometric` distribution table into a side index keyed by `IfcLightFixture` ID — mirrors the existing styling index pattern | Draft |

## Workflow

1. Draft proposal as markdown in this directory.
2. Holger reviews, edits, decides whether to submit.
3. Holger posts to upstream (Issue with `enhancement` label, or
   Discussion under Ideas) from his own GitHub account.
4. If maintainers accept, follow-up PR ports the bimifc implementation
   against the upstream `EntityDecoder` API.

Drafts stay local until Holger explicitly says to post. Public posts
on someone else's repo are reviewed before going live.
