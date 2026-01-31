# bimifc

Pure Rust IFC (Industry Foundation Classes) parser and viewer for BIM applications.

**Website:** [bimifc.de](https://bimifc.de)

## Features

- **Memory-efficient parsing** - Lazy decoding with SIMD-accelerated scanning
- **IFC4 + IFC5 support** - Both STEP and IFCX (JSON) formats
- **Pure Rust** - No JavaScript dependencies, runs natively or as WASM
- **Multiple viewers** - Web (Leptos + Bevy), Terminal (Ratatui)

## Crates

| Crate | Description |
|-------|-------------|
| `bimifc-model` | Trait definitions and shared types |
| `bimifc-parser` | High-performance IFC4/IFC5 parser |
| `bimifc-geometry` | Geometry processing and tessellation |
| `bimifc-bevy` | Bevy-based 3D renderer (WebGPU/WebGL2) |
| `bimifc-viewer-tui` | Terminal viewer with 3D rendering |

*Note: `bimifc-leptos` and `bimifc-viewer` are work-in-progress.*

## Quick Start

```rust
use bimifc_parser::parse_auto;

let content = std::fs::read_to_string("model.ifc")?;
let model = parse_auto(&content)?;

println!("Schema: {}", model.metadata().schema_version);
println!("Entities: {}", model.resolver().entity_count());

// Access spatial hierarchy
if let Some(tree) = model.spatial().spatial_tree() {
    println!("Project: {}", tree.name);
}
```

## Terminal Viewer

```bash
cargo run -p bimifc-viewer-tui -- model.ifc
```

Controls:
- `PgUp/PgDn` - Change floor level
- `+/-` - Zoom
- `WASD` - Pan
- `Tab` - Cycle focus
- `Q` - Quit

## Building

```bash
# Build all crates
cargo build --release

# Run tests
cargo test

# Build TUI viewer
cargo build -p bimifc-viewer-tui --release
```

## License

MPL-2.0
