import SwiftUI

struct SidebarView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        Group {
            if let model = appState.model {
                VStack(spacing: 0) {
                    // Search bar
                    SearchBar(text: $appState.searchText) {
                        appState.search()
                    }
                    .padding(8)

                    if !appState.searchResults.isEmpty {
                        SearchResultsList()
                    } else if let tree = appState.spatialTree {
                        SpatialTreeView(node: tree)
                    } else {
                        EntityListView()
                    }
                }
                .navigationTitle(appState.fileName ?? "BimIFC")
                #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar {
                    #if os(iOS)
                    ToolbarItem(placement: .navigationBarTrailing) {
                        Button {
                            appState.showFilePicker = true
                        } label: {
                            Image(systemName: "doc.badge.plus")
                        }
                    }
                    #endif
                    ToolbarItem(placement: .automatic) {
                        ModelInfoButton()
                    }
                }
            } else {
                WelcomeView()
            }
        }
    }
}

// MARK: - Search

struct SearchBar: View {
    @Binding var text: String
    var onSearch: () -> Void

    var body: some View {
        HStack {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField("Search entities...", text: $text)
                .textFieldStyle(.plain)
                .onSubmit { onSearch() }
            if !text.isEmpty {
                Button {
                    text = ""
                    onSearch()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(8)
        .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))
    }
}

struct SearchResultsList: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        List(appState.searchResults, id: \.self, selection: $appState.selectedEntityId) { id in
            EntityRow(id: id)
        }
    }
}

// MARK: - Spatial tree

struct SpatialTreeView: View {
    let node: SpatialNode
    @EnvironmentObject var appState: AppState

    var body: some View {
        List(selection: $appState.selectedEntityId) {
            SpatialNodeRow(node: node)
        }
    }
}

struct SpatialNodeRow: View {
    let node: SpatialNode
    @EnvironmentObject var appState: AppState

    var body: some View {
        if node.children.isEmpty {
            Label {
                Text(node.name.isEmpty ? node.entityType : node.name)
                    .lineLimit(1)
            } icon: {
                Image(systemName: iconForNodeType(node.nodeType))
                    .foregroundStyle(colorForNodeType(node.nodeType))
            }
            .tag(node.id)
        } else {
            DisclosureGroup {
                ForEach(node.children, id: \.id) { child in
                    SpatialNodeRow(node: child)
                }
            } label: {
                Label {
                    HStack {
                        Text(node.name.isEmpty ? node.entityType : node.name)
                            .lineLimit(1)
                        if let elevation = node.elevation {
                            Text(String(format: "%.1fm", elevation))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                } icon: {
                    Image(systemName: iconForNodeType(node.nodeType))
                        .foregroundStyle(colorForNodeType(node.nodeType))
                }
            }
            .tag(node.id)
        }
    }
}

// MARK: - Entity list (fallback when no spatial tree)

struct EntityListView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        let ids = appState.model?.allEntityIds() ?? []
        List(ids.prefix(500), id: \.self, selection: $appState.selectedEntityId) { id in
            EntityRow(id: id)
        }
    }
}

struct EntityRow: View {
    let id: UInt32
    @EnvironmentObject var appState: AppState

    var body: some View {
        let info = appState.entityInfo(id: id)
        Label {
            VStack(alignment: .leading) {
                Text(appState.entityName(id: id))
                    .lineLimit(1)
                if let info {
                    Text(info.ifcType)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } icon: {
            if let info {
                Image(systemName: iconForEntityType(info.ifcType))
                    .foregroundStyle(colorForEntityType(info.ifcType))
            } else {
                Image(systemName: "doc.text")
                    .foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: - Welcome view

struct WelcomeView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: "building.2")
                .font(.system(size: 64))
                .foregroundStyle(.secondary)
            Text("BimIFC Viewer")
                .font(.title)
            Text("Open an IFC file to get started")
                .foregroundStyle(.secondary)
            Button("Open File...") {
                appState.showFilePicker = true
            }
            .buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Model info

struct ModelInfoButton: View {
    @EnvironmentObject var appState: AppState
    @State private var showPopover = false

    var body: some View {
        Button {
            showPopover.toggle()
        } label: {
            Image(systemName: "info.circle")
        }
        .popover(isPresented: $showPopover) {
            if let meta = appState.metadata {
                ModelInfoView(metadata: meta, entityCount: appState.model?.entityCount() ?? 0)
                    .frame(minWidth: 280)
                    .padding()
            }
        }
    }
}

struct ModelInfoView: View {
    let metadata: ModelMetadata
    let entityCount: UInt32

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Model Information")
                .font(.headline)
            Divider()
            InfoRow(label: "Schema", value: metadata.schemaVersion)
            InfoRow(label: "Entities", value: "\(entityCount)")
            if let system = metadata.originatingSystem {
                InfoRow(label: "Application", value: system)
            }
            if let author = metadata.author {
                InfoRow(label: "Author", value: author)
            }
            if let org = metadata.organization {
                InfoRow(label: "Organization", value: org)
            }
            if let name = metadata.fileName {
                InfoRow(label: "File", value: name)
            }
        }
    }
}

struct InfoRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .foregroundStyle(.secondary)
                .frame(width: 100, alignment: .trailing)
            Text(value)
        }
        .font(.callout)
    }
}

// MARK: - Helpers

func iconForNodeType(_ nodeType: String) -> String {
    switch nodeType {
    case "Project": return "folder"
    case "Site": return "map"
    case "Building": return "building.2"
    case "Storey": return "square.split.1x2"
    case "Space": return "square.dashed"
    case "Element": return "cube"
    case "Facility": return "road.lanes"
    case "FacilityPart": return "road.lanes.curved.right"
    default: return "questionmark.square"
    }
}

func colorForNodeType(_ nodeType: String) -> Color {
    switch nodeType {
    case "Project": return .purple
    case "Site": return .green
    case "Building": return .blue
    case "Storey": return .orange
    case "Space": return .cyan
    case "Element": return .gray
    default: return .secondary
    }
}

func iconForEntityType(_ ifcType: String) -> String {
    let upper = ifcType.uppercased()
    if upper.contains("WALL") { return "rectangle.split.3x1" }
    if upper.contains("SLAB") { return "square.fill" }
    if upper.contains("ROOF") { return "house.fill" }
    if upper.contains("WINDOW") { return "window.vertical.open" }
    if upper.contains("DOOR") { return "door.left.hand.open" }
    if upper.contains("BEAM") || upper.contains("COLUMN") { return "line.diagonal" }
    if upper.contains("STAIR") { return "stairs" }
    if upper.contains("RAILING") { return "fence" }
    if upper.contains("LIGHTFIXTURE") { return "lightbulb.fill" }
    if upper.contains("FURNISHING") || upper.contains("FURNITURE") { return "chair.fill" }
    if upper.contains("SPACE") { return "square.dashed" }
    if upper.contains("FLOW") || upper.contains("PIPE") { return "pipe.and.drop" }
    if upper.contains("COVERING") { return "paintbrush" }
    return "cube"
}

func colorForEntityType(_ ifcType: String) -> Color {
    let upper = ifcType.uppercased()
    if upper.contains("WALL") { return .brown }
    if upper.contains("SLAB") { return .gray }
    if upper.contains("ROOF") { return .orange }
    if upper.contains("WINDOW") { return .cyan }
    if upper.contains("DOOR") { return .brown }
    if upper.contains("LIGHTFIXTURE") { return .yellow }
    if upper.contains("SPACE") { return .blue }
    return .secondary
}
