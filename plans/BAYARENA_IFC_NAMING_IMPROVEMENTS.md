# BayArena IFC — Naming & Structure Improvements

## Current State Analysis

### What's there now

| Entity | Current Name | Issues |
|--------|-------------|--------|
| `IfcProject` | `BayArena Lighting Demo` | ✅ Fine |
| `IfcSite` | `BayArena Site` | ⚠️ Could include city/address |
| `IfcBuilding` | `BayArena` | ⚠️ Missing Description |
| `IfcBuildingStorey` | `Ground Level` | ⚠️ Only one storey, no roof level |
| `IfcBuildingElementProxy` | `Stadium Structure` | ⚠️ Generic, no ObjectType |
| `IfcRoof` | `Stadium Roof` | ✅ Acceptable |
| `IfcSlab` | `Playing Field` | ✅ Acceptable |
| `IfcLightFixtureType` | `LEDVANCE FL Arena` | ⚠️ Missing model variant |
| 324× `IfcLightFixture` | `FL Arena Ring1 #1` … `#108` | ❌ Major problems |

### Fixture Naming Problems

All 324 fixtures follow `FL Arena Ring{1-3} #{1-108}` — three issues:

1. **No spatial orientation** — `#47` tells you nothing about *where* on the ring it sits
2. **No stadium side reference** — East Stand? West Stand? Goal End North?
3. **No functional zone** — Is this lighting the pitch, the stands, or doing broadcast fill?
4. **Sequential numbering is meaningless** — without a mapping table, nobody can locate fixture #73
5. **Description field (`$`)** — completely empty on every fixture
6. **ObjectType field (`$`)** — empty, should carry the luminaire variant

---

## Recommended Naming Scheme

### 1. Fixture Naming Convention

**Pattern:**
```
{Product}-{Ring}-{Side}-{Index}
```

**Examples:**
```
FLA-R1-EAST-01      → FL Arena, Ring 1, East Stand, fixture 1
FLA-R2-NORTH-12     → FL Arena, Ring 2, North Goal End, fixture 12
FLA-R3-WEST-05      → FL Arena, Ring 3, West Stand, fixture 5
FLA-R1-SOUTH-08     → FL Arena, Ring 1, South Goal End, fixture 8
```

**Side mapping for BayArena (oval layout):**
- `EAST` — Main Stand (Haupttribüne), facing cameras
- `WEST` — Opposite Stand (Gegentribüne)  
- `NORTH` — North Goal End (Nordkurve)
- `SOUTH` — South Goal End (Südkurve)
- `NE`, `NW`, `SE`, `SW` — Corner transitions

Since the BayArena has an oval ring (not a rectangular truss), you can also divide the 108 fixtures per ring into **angular sectors** based on their position angle:

```
FLA-R1-000-01   → Ring 1, 0° (North), fixture 1
FLA-R1-090-27   → Ring 1, 90° (East), fixture 27
FLA-R1-180-54   → Ring 1, 180° (South), fixture 54
FLA-R1-270-81   → Ring 1, 270° (West), fixture 81
```

### 2. Use the Description Field

Currently every fixture has `$` (empty) for Description. This is wasted real estate.

**Recommendation:**
```
IFCLIGHTFIXTURE('guid',$,
  'FLA-R1-EAST-01',                           ← Name
  'Ring 1, East Stand, Position 1 of 27',     ← Description  
  'LEDVANCE FL Arena 1200W',                  ← ObjectType
  ...);
```

The Description should contain human-readable location info. This is what appears in BIM viewer tooltips and schedule exports.

### 3. Populate ObjectType

The 5th positional parameter (`ObjectType`) is empty on all fixtures. It should carry the product identifier:

```
ObjectType = 'LEDVANCE FL Arena 1200W 5700K'
```

This lets BIM tools filter/schedule by product variant even without querying property sets.

### 4. Spatial Structure Improvements

#### Add a Roof Level Storey
Fixtures are mounted at roof level (~40m), not ground level. Add:

```
IFCBUILDINGSTOREY('guid',$,'Roof Ring Level','Lighting ring structure at +40.0m',$,...);
```

Then assign fixtures to this storey instead of `Ground Level`.

#### Enrich IfcSite
```
Current:  IFCSITE('guid',$,'BayArena Site',$,$,...);
Improved: IFCSITE('guid',$,'BayArena','Bismarckstraße 122-124, 51373 Leverkusen',$,...);
```

#### Add IfcSpace for Pitch Area
```
IFCSPACE('guid',$,'Pitch','UEFA regulation playing field 105×68m',...);
```

### 5. Group Fixtures with IfcGroup or IfcSystem

Currently all 324 fixtures sit in one flat `IfcRelContainedInSpatialStructure` — no grouping at all.

**Add `IfcSystem` per ring:**
```step
#G1=IFCSYSTEM('guid',$,'Lighting Ring 1','Inner ring - narrow beam aiming',$,$);
#G2=IFCSYSTEM('guid',$,'Lighting Ring 2','Middle ring - medium beam',$,$);  
#G3=IFCSYSTEM('guid',$,'Lighting Ring 3','Outer ring - wide flood fill',$,$);
```

Connect with `IfcRelAssignsToGroup`:
```step
IFCRELASSIGNSTOGROUP('guid',$,'Ring 1 Assignment',$,
  (#88,#98,#107,...108 fixtures...),$,#G1);
```

**Optionally add IfcGroup per stadium side:**
```step
#GE=IFCGROUP('guid',$,'East Stand Fixtures','Main stand lighting cluster',$);
```

### 6. Property Set Improvements

#### Current Issues
- `Wattage` is `IFCLABEL('1200W')` — should be `IFCPOWERMEASURE(1200.0)` with unit
- `Lumens` is `IFCLABEL('71000')` — should be `IFCLUMINOUSFLUXMEASURE(71000.0)`
- `CCT` is `IFCLABEL('5700K')` — should be numeric
- `CRI` is `IFCLABEL('80')` — should be `IFCREAL(80.0)`
- `BeamAngle` as string `'15-60°'` loses min/max semantics

#### Recommended Fix
```step
#78=IFCPROPERTYSINGLEVALUE('NominalPower',$,IFCPOWERMEASURE(1200.0),$);
#79=IFCPROPERTYSINGLEVALUE('LuminousFlux',$,IFCLUMINOUSFLUXMEASURE(71000.0),$);  
#80=IFCPROPERTYSINGLEVALUE('ColourTemperature',$,IFCTHERMODYNAMICTEMPERATUREMEASURE(5700.0),$);
#81=IFCPROPERTYSINGLEVALUE('ColourRenderingIndex',$,IFCREAL(80.0),$);
```

For beam angle, use two properties:
```step
IFCPROPERTYSINGLEVALUE('BeamAngleMin',$,IFCPLANEANGLEMEASURE(0.2618),$);  -- 15° in rad
IFCPROPERTYSINGLEVALUE('BeamAngleMax',$,IFCPLANEANGLEMEASURE(1.0472),$);  -- 60° in rad
```

### 7. IfcLightFixtureType Improvements

```
Current:  IFCLIGHTFIXTURETYPE('guid',$,'LEDVANCE FL Arena',$,$,...);
Improved: IFCLIGHTFIXTURETYPE('guid',$,
  'LEDVANCE FL Arena 1200W',                              ← Name
  'High-power LED floodlight for sports venue lighting',  ← Description
  'urn:ledvance:product:fl-arena-1200',                   ← Tag
  ...,.DIRECTIONSOURCE.);
```

---

## Implementation in the Import Script

### Fixture Name Generation (pseudocode)

```python
SIDES = ['NORTH', 'NE', 'EAST', 'SE', 'SOUTH', 'SW', 'WEST', 'NW']

def fixture_name(ring: int, index: int, total_per_ring: int = 108) -> tuple[str, str]:
    """Returns (Name, Description) for an IfcLightFixture."""
    
    # Calculate angular position
    angle_deg = (index / total_per_ring) * 360.0
    
    # Determine stadium side from angle
    sector_index = int((angle_deg + 22.5) % 360 / 45)
    side = SIDES[sector_index]
    
    # Count within this side segment
    side_start = sector_index * (total_per_ring // 8)
    local_index = index - side_start + 1
    
    name = f"FLA-R{ring}-{side}-{local_index:02d}"
    description = (
        f"Ring {ring}, {side} sector, "
        f"position {index+1}/{total_per_ring}, "
        f"angle {angle_deg:.1f}°"
    )
    object_type = "LEDVANCE FL Arena 1200W 5700K"
    
    return name, description, object_type
```

**Output examples:**
```
FLA-R1-NORTH-01   "Ring 1, NORTH sector, position 1/108, angle 0.0°"
FLA-R1-EAST-14    "Ring 1, EAST sector, position 27/108, angle 90.0°"  
FLA-R2-SOUTH-01   "Ring 2, SOUTH sector, position 55/108, angle 183.3°"
FLA-R3-WEST-07    "Ring 3, WEST sector, position 81/108, angle 270.0°"
```

---

## Priority Order

| # | Change | Effort | Impact |
|---|--------|--------|--------|
| 1 | **Fixture names** → `FLA-R{n}-{SIDE}-{idx}` | Low | 🔴 High — this is the main pain point |
| 2 | **Fill Description** on every fixture | Low | 🔴 High — BIM viewer UX |
| 3 | **Fill ObjectType** on every fixture | Trivial | 🟡 Medium |
| 4 | **Add IfcSystem per ring** | Medium | 🟡 Medium — enables ring-level scheduling |
| 5 | **Typed property values** (power, lumens) | Medium | 🟡 Medium — enables quantity takeoff |
| 6 | **Add Roof Level storey** | Low | 🟢 Nice-to-have |
| 7 | **Enrich Site address** | Trivial | 🟢 Nice-to-have |
| 8 | **IfcGroup per side** | Medium | 🟢 Nice-to-have |

Items 1–3 are pure string changes in the fixture generation loop. Items 4–5 require new IFC entities. Items 6–8 are structural additions.
