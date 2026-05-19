# Plan: `bimifc-egui` — native desktop viewer

**Status:** Post-demo. Not started.
**Goal:** Native macOS/Linux/Windows desktop app that uses the same
parser + geometry pipeline as the web viewer, with an egui UI for
hierarchy, properties, and discipline filtering.

## Why not just use the web viewer?

Three things native gives us that the wasm path can't:
1. **No 200 MB parse-and-serialize through localStorage.** Native reads
   `./ifc/foo.ifc` directly into the parser. The 198 MB medical clinic
   already loads in 2.3 s parse + 843 ms tessellate (see `bench_medical`).
2. **Full memory headroom.** Wasm32 sandboxes at 4 GB; the medical
   clinic peaks at 1.7 GB native and would clip in the browser without
   streaming.
3. **No browser quirks.** No COOP/COEP, no SharedArrayBuffer dance,
   no wasm-bindgen tax on JS bridging.

## Why egui and not Bevy-UI?

Bevy already drives our 3D viewport on both web and native. Bevy-UI
exists and works fine for HUD/controls, but it's tuned for game UI:
custom widgets, layout system that's not a typical desktop CRUD shape.
Egui is purpose-built for tooling — tree views, properties panels,
collapsing headers, drag-and-drop tables. The hierarchy + properties
side of bimifc is dominantly that.

Architecturally:
- **3D viewport (centre)**: Bevy renders into an `egui::TextureHandle`
  using `bevy_egui` (or a custom paint callback into wgpu). We reuse
  100% of `bimifc-bevy`'s scene + lighting + picking code.
- **Side panels (left/right)**: pure egui. Hierarchy tree, properties
  panel, toolbar, status bar.
- **Top toolbar**: egui buttons for the same `MepView` filter we have
  in leptos.

## Layout sketch

```
┌─────────────────────────────────────────────────────────────────────┐
│ File  View  Help          [🏗️][🏛️][⚡][🔧][💨][💡]   🌗 ⌨ LISP    │
├──────────────────┬──────────────────────────────────────────────────┤
│ Hierarchy        │                                                  │
│ ▾ Project        │                                                  │
│   ▾ Site         │              3D viewport (Bevy)                  │
│     ▾ Building   │                                                  │
│       ▸ Storey 1 │                                                  │
│       ▸ Storey 2 │                                                  │
│                  │                                                  │
├──────────────────┴──────────────────────────────────────────────────┤
│ Properties                                                          │
│ Name: Pipe Types: Waste                                             │
│ GlobalId: 2BCdEf...                                                 │
│ Pset_PipeSegmentTypeCommon                                          │
│   NominalDiameter: 50 mm                                            │
│   Material: PVC                                                     │
├─────────────────────────────────────────────────────────────────────┤
│ Loaded: NBU_MedicalClinic_Eng-MEP.ifc — 297K tri | All disciplines │
└─────────────────────────────────────────────────────────────────────┘
```

## Crate structure

```
crates/bimifc-egui/
  Cargo.toml
  src/
    main.rs        — eframe::App entry
    app.rs         — App state + update loop
    viewport.rs    — Bevy ↔ egui texture bridge
    hierarchy.rs   — left panel tree widget
    properties.rs  — right panel
    toolbar.rs     — top toolbar with MepView buttons
    state.rs       — shared signals: selection, visibility, mep_view, etc.
```

Deps:
- `eframe = "0.30"` (or whatever current is at port-time)
- `egui = "0.30"`
- `bimifc-bevy = { path = "..." }` for 3D
- `bimifc-parser`, `bimifc-model`, `bimifc-geometry`
- `bimifc-lisp` (later, behind a `lisp-panel` feature)
- `rfd` for file open dialogs (we already use it elsewhere)

## Sharing code with the leptos viewer

Two things are worth factoring out before the egui port:
1. **MepView + classify_by_name**: currently lives in
   `crates/bimifc-leptos/src/state.rs`. Move to `bimifc-model`
   (or a new `bimifc-ui-core` crate) so both leptos and egui consume
   the same enum + matcher. Saves us drift between two copies of
   the same keyword list.
2. **`ViewFilter` from `bimifc-viewer-tui`**: same shape, different
   crate. Could move to `bimifc-model` too. Or just live with two
   parallel definitions for now — they'll stay in sync if the
   keyword list is centralised.

## Open questions for when we start

- Does `bevy_egui` mature enough on Bevy 0.18? Last I checked the crate
  lags Bevy versions by a release. If it's behind 0.18, the alternative
  is `egui-wgpu` directly with our own wgpu surface; more code but
  uncouples from `bevy_egui`'s release schedule.
- Tauri vs. eframe for window shell? eframe is simpler; Tauri is what
  the upstream ifc-lite uses for their desktop variant. For bimifc-only
  use, eframe is enough.
- Do we want the lisp REPL in this app or strictly the viewer? Probably
  yes, behind a feature flag.

## Effort estimate

- Walking skeleton (window opens, IFC loads, 3D shows): **half a day**
- Hierarchy + properties + discipline buttons (parity with the leptos
  web viewer minus the lisp panel): **another day**
- Polish, file dialog, view persistence, keyboard shortcuts: **half a day**

So ~2 productive days for a feature-complete viewer.

## When to start

After tomorrow's demo. The web demo + TUI demo both work; egui is
additive, not a blocker.
