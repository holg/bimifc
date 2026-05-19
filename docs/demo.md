# Demo cheat sheet — bimifc MEP disciplines

Date target: 2026-05-20 demo.

## Demo file

**`ifc/NBU_MedicalClinic_Eng-MEP.ifc`** — 198 MB IFC2x3 from a real
medical clinic project. Contains 3.2M entities, 16,539 geometry-eligible
products, and renders to ~297K triangles.

Why this file: it's a real-world MEP-heavy model. The story is
*"bimifc handles 200 MB on a laptop, classifies disciplines including
on legacy IFC2x3 files where the type information is too generic"*.

## Numbers worth quoting

- **Parse**: 197.7 MB read in 29 ms, 3.2M entities parsed in 2.3 s
- **Tessellate** (`Scene::from_content`): 843 ms after parse
- **Peak memory**: 1.7 GB native
- **Discipline classification** on the 297K triangles:
  - Other (walls/floors/structure): 287,594 (96.7%)
  - Plumbing: 8,372 (2.8%)
  - HVAC: 1,176 (0.4%)
  - Electrical: 336 (0.1%)
  - Lighting: 0 (the 9 IfcLightFixtureType entries in this file don't
    carry body representations — they're type templates, not placed
    instances. Use bayarena_lighting.ifc for the lighting demo.)

## Three viewers, three angles

### 1. Native TUI (recommended for "speed + discipline filter" story)

```bash
cargo run --release -p bimifc-viewer-tui -- ifc/NBU_MedicalClinic_Eng-MEP.ifc
```

Keys to demo, in order:
- `f` — fit all (initial frame)
- `↑ ↓ ← →` — orbit camera
- `+` / `-` — zoom
- `b` — **Architecture only** — hides ALL MEP, shows just the shell
- `p` — **Plumbing only** (status bar shows "8372 / 297478 triangles (2.8%)")
- `m` — **HVAC only** ("1176 / 297478 triangles (0.4%)")
- `e` — **Electrical only** ("336 / 297478 triangles (0.1%)")
- `l` — **Lighting only** (0 here; for this file, switch story to bayarena)
- `x` — show All again (everything back)
- `v` — cycle Iso3D / FloorPlan / Polar / BlockChar
- `q` — quit

The discipline-filter status message in the status bar names the
percentage live — handy talking point.

### 2. Web viewer (Leptos + Bevy via wasm32)

```bash
cd crates/bimifc-viewer && trunk serve
# Open http://127.0.0.1:8083/?file=NBU_MedicalClinic_Eng-MEP.ifc
```

The 198 MB file will take noticeably longer to load in the browser
than native (browser parses string + serializes geometry through
localStorage). For a smoother demo, use a smaller file:

```
http://127.0.0.1:8083/?file=bayarena_lighting.ifc
http://127.0.0.1:8083/?file=AZB%20office.ifc
```

Toolbar buttons (left to right after the camera/section controls):
- 💡 Toggle lighting mode (architectural / photometric / combined)
- 🏗️ All disciplines
- 🏛️ Architecture only (hides all MEP)
- ⚡ Electrical only
- 🔧 Plumbing only
- 💨 HVAC only
- 💡 Lighting only

For the bayarena file the Lighting button is the headliner — 327 light
fixtures with embedded EULUMDAT photometric data, polar diagrams in
the property panel.

### 3. Native bench (proof-of-speed talking point)

```bash
cargo run --release -p bimifc-parser --example bench_medical -- \
    ifc/NBU_MedicalClinic_Eng-MEP.ifc
```

Prints parse time / entity counts / MEP totals / memory in 6 seconds.
Useful to show "before any tessellation, we've already cracked the
file" if someone questions whether 198 MB is real.

## Talking points for the discipline filter

1. **Inheritance graph** (from `bimifc-model 0.3.0`, today's release):
   `IfcCableSegment` inherits from `IfcFlowSegment` via our
   hand-maintained `parent()` table. Adding a new MEP subtype only
   requires one arm in `parent()` — `has_geometry()` and the spatial
   geometry cache pick it up automatically. Concrete demo: the 7 types
   we added yesterday (cable/pipe/HVAC) work without enumerating them
   in any leaf-list.

2. **Name-fallback for IFC2x3** (today's TUI + Leptos work): the
   medical clinic is from 2011 and uses `IfcFlowSegment` for ducts,
   pipes, and cables alike — the type alone can't tell us discipline.
   Solution: when the type-based classifier returns `Other`, fall back
   to keyword matching on the entity's `Name` attribute. Real Revit
   exports name them helpfully ("Rectangular Duct" → HVAC, "Pipe Types:
   Waste" → Plumbing, "Troffer Light" → Lighting). The bench shows the
   classifier finding 9,884 MEP triangles in a file where the type
   index alone would have shown 0.

## If something goes wrong

- **Viewer hangs in browser on 198 MB load**: that's the wasm32 path
  bottleneck (file → JS string → leptos buffer → bevy serialize). Pivot
  to the TUI demo, mention the bottleneck is a known optimization
  target. Native + wasm parser are the same code, only the bridge layer
  differs.
- **Trunk hot-reload broke a feature mid-demo**: in another terminal,
  `cd crates/bimifc-viewer && trunk build` to force a clean rebuild
  before re-running.
- **`bimifc-tui` doesn't render colors**: terminal needs truecolor.
  Modern iTerm2 / Apple Terminal / Alacritty handle it. SSH sessions
  may need `COLORTERM=truecolor`.
