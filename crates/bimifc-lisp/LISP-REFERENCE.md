# bimifc-lisp — AutoLISP IFC Reference

AutoLISP scripting engine for creating, querying, and visualizing IFC BIM models.

## Quick Start

```lisp
; Create a simple room
(ifc-new)
(ifc-set-project "My House" "Site" "Building" "Ground" "Author" "Org")
(ifc-storey "Ground Floor" 0.0)
(ifc-add-wall 0 0 5 0.3 2.8 "North Wall")
(ifc-add-wall 0 0 0.3 4 2.8 "West Wall")
(ifc-add-wall 4.7 0 0.3 4 2.8 "East Wall")
(ifc-add-wall 0 3.7 5 0.3 2.8 "South Wall")
(ifc-add-door 1.5 0 1.0 0.3 2.1 "Door")
(ifc-add-window 3 3.7 1.2 0.3 1.5 "Window")
(ifc-save "room.ifc")
```

## Units

All dimensions are in **meters**. Coordinates use a right-hand system:
- **X** = width (east-west)
- **Y** = depth (north-south)
- **Z** = height (up), specified via `z` offset parameter

## Creating Models

### Project Setup

| Function | Description |
|----------|-------------|
| `(ifc-new)` | Clear all elements and start a fresh model |
| `(ifc-set-project name site building storey author org)` | Set project metadata (all strings) |
| `(ifc-storey "name" elevation)` | Set current storey for subsequent elements. Elevation in meters |
| `(ifc-color r g b)` | Set surface color for subsequent elements (RGB 0.0–1.0) |
| `(ifc-no-color)` | Clear current color, revert to viewer palette defaults |

### Building Elements

All element functions follow the pattern:
```
(ifc-add-TYPE x y width depth height [z] ["name"]) → local-id
```

**Parameters:**
- `x y` — position of the element's bottom-left corner (meters)
- `width depth` — footprint size (meters)
- `height` — extrusion height (meters), 0 = use default
- `z` — optional elevation offset (meters), default 0.0
- `"name"` — optional label string

If `z` is omitted and the next argument is a string, it's treated as the name.

| Function | IFC Type | Default Height | Description |
|----------|----------|---------------|-------------|
| `(ifc-add-wall ...)` | IfcWallStandardCase | 2.8m | Exterior or interior wall |
| `(ifc-add-door ...)` | IfcDoor | 2.1m | Door opening |
| `(ifc-add-window ...)` | IfcWindow | 1.5m | Window opening |
| `(ifc-add-slab ...)` | IfcSlab | 0.3m | Floor slab or ceiling |
| `(ifc-add-roof ...)` | IfcRoof (GABLE) | 3.0m | Pitched Satteldach — height = ridge height |
| `(ifc-add-column ...)` | IfcColumn | 2.8m | Structural column |
| `(ifc-add-beam ...)` | IfcBeam | 0.4m | Structural beam |
| `(ifc-add-stair ...)` | IfcStair | 2.8m | Staircase volume |
| `(ifc-add-railing ...)` | IfcRailing | 1.0m | Handrail or balustrade |
| `(ifc-add-furniture x y w d h "category" ["name"])` | IfcFurnishingElement | 0.8m | Furniture — see categories below |

**Furniture categories:** bed, table, chair, sofa, storage, toilet, bathtub, sink, refrigerator, stove, oven, washerdryer, dishwasher, television, fireplace

### Colors & Materials

Use `(ifc-color r g b)` to set RGB color (0.0–1.0) for all subsequent elements. The color generates the full IFC style chain: `IFCCOLOURRGB` → `IFCSURFACESTYLERENDERING` → `IFCSURFACESTYLE` → `IFCSTYLEDITEM`.

```lisp
(ifc-color 0.85 0.45 0.35)              ; terracotta red
(ifc-add-roof 0 0 10 8 3.0 6.0 "Roof")  ; will render in terracotta

(ifc-color 0.95 0.90 0.80)              ; warm cream
(ifc-add-wall 0 0 10 0.3 2.8 "Wall")    ; will render in cream

(ifc-no-color)                           ; back to viewer defaults
(ifc-add-slab 0 0 10 8 0.3 "Slab")      ; uses palette color
```

**Common colors** (RGB values):
| Material | R | G | B |
|----------|---|---|---|
| White plaster | 0.95 | 0.93 | 0.88 |
| Brick red | 0.72 | 0.30 | 0.22 |
| Concrete grey | 0.75 | 0.75 | 0.73 |
| Wood brown | 0.55 | 0.35 | 0.18 |
| Roof tile (Dachziegel) | 0.65 | 0.32 | 0.22 |
| Glass blue | 0.60 | 0.78 | 0.90 |
| Steel | 0.60 | 0.62 | 0.65 |
| Grass green | 0.30 | 0.55 | 0.25 |

### Editing

| Function | Description |
|----------|-------------|
| `(ifc-remove id)` | Remove element by local ID → T or Nil |
| `(ifc-move id dx dy)` | Move element by offset → T or Nil |

### Saving

| Function | Description |
|----------|-------------|
| `(ifc-save "path.ifc")` | Write IFC 2x3 STEP file to disk |

## Multi-Storey Buildings

Use `(ifc-storey)` to define storeys with explicit elevations. All subsequent `ifc-add-*` calls are assigned to the current storey.

```lisp
(ifc-storey "Ground Floor" 0.0)     ; elevation 0.0m
(ifc-add-slab 0 0 10 8 0.3 0 "Ground Slab")
(ifc-add-wall 0 0 10 0.3 2.8 0.3 "North Wall")  ; z=0.3 (on top of slab)

(ifc-storey "First Floor" 3.1)      ; elevation 3.1m
(ifc-add-slab 0 0 10 8 0.3 3.1 "1F Slab")
(ifc-add-wall 0 0 10 0.3 2.6 3.4 "1F Wall")      ; z=3.4 (on top of 1F slab)

(ifc-storey "Roof" 6.0)             ; elevation 6.0m
(ifc-add-roof 0 0 10 8 3.0 6.0 "Satteldach")
```

**Dimensional stack** for typical German construction:
```
z=0.0   Bodenplatte (slab, 0.3m thick)
z=0.3   EG walls start (2.8m high → top at 3.1m)
z=3.1   OG Decke (slab, 0.3m thick)
z=3.4   OG walls start (2.6m high → top at 6.0m)
z=6.0   Dach / Roof
```

## Querying Loaded Models

### Loading

| Function | Description |
|----------|-------------|
| `(ifc-load "path.ifc")` | Load and parse an IFC file → T or Nil |

### Entity Queries

| Function | Returns |
|----------|---------|
| `(ifc-entity-count)` | Integer — total entity count |
| `(ifc-entities)` | List of all entity IDs |
| `(ifc-entities-by-type "IfcWall")` | List of IDs matching type |
| `(ifc-entity-type id)` | String — e.g. "IfcWall" |
| `(ifc-entity-name id)` | String or Nil |
| `(ifc-search "query")` | List of IDs matching name/type search |

### Properties

| Function | Returns |
|----------|---------|
| `(ifc-property-sets id)` | Nested list: `(("PsetName" ("key" "val") ...) ...)` |
| `(ifc-get-property id "PropName")` | String value or Nil |
| `(ifc-quantities id)` | List: `(("name" value "unit") ...)` |

### Spatial Structure

| Function | Returns |
|----------|---------|
| `(ifc-storeys)` | List: `((id "name" elevation) ...)` |
| `(ifc-elements-in-storey storey-id)` | List of element IDs |
| `(ifc-spatial-tree)` | Nested: `(id "type" "name" (children...))` |
| `(ifc-metadata)` | List: `(("schema" "IFC4") ("system" "Revit") ...)` |

## Drawing & Visualization

### 2D Plan View

| Function | Returns |
|----------|---------|
| `(ifc-draw id)` | Integer — line count for one entity |
| `(ifc-draw-all)` | Integer — line count for all entities |
| `(ifc-draw-storey storey-id)` | Integer — line count for one storey |
| `(ifc-draw-svg "path.svg")` | T — exports drawing as SVG file |

Lines are placed on layers matching IFC types (IfcWall, IfcDoor, etc.) for color grouping in SVG output.

## WASM / Web Viewer

In the bimifc web viewer, the LISP REPL panel provides:
- **Run** (green button / Ctrl+Enter) — execute code, show SVG plan view
- **3D** (blue button / Ctrl+Shift+Enter) — execute + send to Bevy 3D viewer
- **Example** — load the demo house script
- **Clear** — reset all state

The 3D button generates an IFC file from the writer elements and feeds it through the same parsing pipeline as file upload — the model appears in the 3D viewport with full interaction (orbit, pan, select, hide/isolate).

## CLI Usage

```bash
bimifc-lisp                              # REPL mode
bimifc-lisp script.lsp                   # Run a script file
bimifc-lisp --ifc model.ifc              # REPL with pre-loaded model
bimifc-lisp --ifc model.ifc script.lsp   # Pre-load + run script
```

## Examples

### Export plan view of existing IFC
```lisp
(ifc-load "building.ifc")
(ifc-draw-all)
(ifc-draw-svg "plan.svg")
```

### Analyze model
```lisp
(ifc-load "building.ifc")
(princ (strcat "Entities: " (itoa (ifc-entity-count))))
(princ (strcat "Storeys: " (itoa (length (ifc-storeys)))))
(foreach wall (ifc-entities-by-type "IfcWall")
  (princ (strcat "  Wall: " (ifc-entity-name wall)))
)
```

### Parametric room generator
```lisp
(defun make-room (x y w d h name)
  (ifc-add-wall x y w 0.2 h (strcat name " North"))
  (ifc-add-wall x y 0.2 d h (strcat name " West"))
  (ifc-add-wall (+ x (- w 0.2)) y 0.2 d h (strcat name " East"))
  (ifc-add-wall x (+ y (- d 0.2)) w 0.2 h (strcat name " South"))
)

(ifc-new)
(ifc-set-project "Office" "Site" "Building" "GF" "Arch" "Firm")
(ifc-storey "Ground Floor" 0.0)
(ifc-add-slab 0 0 20 10 0.3 0 "Floor")
(make-room 0 0 10 10 2.8 "Room A")
(make-room 10 0 10 10 2.8 "Room B")
(ifc-save "office.ifc")
```
