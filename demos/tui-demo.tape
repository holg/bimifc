# VHS Demo: bimifc-tui — Terminal IFC Viewer
#
# Record with: vhs demos/tui-demo.tape
# Produces: demos/tui-demo.gif + demos/tui-demo.mp4
#
# Prerequisites:
#   cargo build --release -p bimifc-viewer-tui

Output demos/tui-demo.gif
Output demos/tui-demo.mp4

# Terminal settings
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms
Set Padding 10

# --- Scene 1: Launch with GLDF-exported IFC file ---
Type "bimifc-tui crates/bimifc-viewer/ifc/gldf-exported.ifc"
Enter
Sleep 3s

# --- Scene 2: 3D orbit view — rotate around the model ---
# Focus starts on viewport, arrow keys control orbit
Sleep 500ms
Right
Sleep 200ms
Right
Sleep 200ms
Right
Sleep 200ms
Up
Sleep 200ms
Up
Sleep 200ms
Left
Sleep 200ms
Left
Sleep 200ms
Left
Sleep 200ms
Down
Sleep 200ms
Down
Sleep 1s

# --- Scene 3: Switch to floor plan view ---
Type "v"
Sleep 2s

# Zoom in
Type "+"
Sleep 300ms
Type "+"
Sleep 300ms
Type "+"
Sleep 1s

# --- Scene 4: Switch focus to hierarchy (Tab) and navigate to light fixture ---
Tab
Sleep 500ms

# Navigate tree: Project(expanded) → Site → Building → Storey → LightFixture
Down
Sleep 400ms
Enter
Sleep 400ms
Down
Sleep 400ms
Enter
Sleep 400ms
Down
Sleep 400ms
Enter
Sleep 400ms
# Select first IfcLightFixture
Down
Sleep 2s

# --- Scene 5: Switch to polar diagram view ---
# Esc to viewport focus, then v → Polar (FloorPlan → Polar is one v press)
Escape
Sleep 300ms
Type "v"
Sleep 3s

# --- Scene 6: Navigate to second fixture (polar updates live) ---
Tab
Sleep 300ms
Down
Sleep 2s

# --- Scene 7: Third fixture ---
Down
Sleep 2s

# --- Scene 8: Back to first fixture ---
Up
Sleep 500ms
Up
Sleep 1s

# --- Scene 9: Return to 3D orbit view ---
# From Polar: v → BlockChar → v → Iso3D (skip block quickly)
Escape
Sleep 200ms
Type "v"
Sleep 100ms
Type "v"
Sleep 1s

# Final orbit
Right
Sleep 200ms
Right
Sleep 200ms
Up
Sleep 200ms
Up
Sleep 1500ms

# --- Exit ---
Type "q"
Sleep 500ms
