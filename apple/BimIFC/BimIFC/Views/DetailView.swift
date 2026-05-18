import SwiftUI

struct DetailView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        if let id = appState.selectedEntityId {
            EntityDetailView(entityId: id)
        } else if appState.model != nil {
            VStack(spacing: 12) {
                Image(systemName: "hand.point.left")
                    .font(.system(size: 48))
                    .foregroundStyle(.secondary)
                Text("Select an entity from the sidebar")
                    .foregroundStyle(.secondary)
            }
        } else {
            WelcomeView()
        }
    }
}

struct EntityDetailView: View {
    let entityId: UInt32
    @EnvironmentObject var appState: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // Header
                if let info = appState.entityInfo(id: entityId) {
                    EntityHeaderView(info: info, name: appState.entityName(id: entityId))
                }

                // GlobalId
                if let globalId = appState.model?.entityGlobalId(id: entityId) {
                    Section("Global ID") {
                        Text(globalId)
                            .font(.system(.body, design: .monospaced))
                            .textSelection(.enabled)
                    }
                }

                // Description
                if let desc = appState.model?.entityDescription(id: entityId) {
                    Section("Description") {
                        Text(desc)
                    }
                }

                // Property sets
                let psets = appState.propertySets(for: entityId)
                if !psets.isEmpty {
                    ForEach(psets, id: \.name) { pset in
                        PropertySetSection(pset: pset)
                    }
                }

                // Quantities
                let quantities = appState.quantities(for: entityId)
                if !quantities.isEmpty {
                    Section("Quantities") {
                        ForEach(quantities, id: \.name) { q in
                            HStack {
                                Text(q.name)
                                Spacer()
                                Text(String(format: "%.2f %@", q.value, q.unit))
                                    .foregroundStyle(.secondary)
                            }
                            .font(.callout)
                        }
                    }
                }
            }
            .padding()
        }
        .navigationTitle("#\(entityId)")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
    }
}

struct EntityHeaderView: View {
    let info: EntityInfo
    let name: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(name)
                .font(.title2.bold())
            HStack {
                Label(info.ifcType, systemImage: info.hasGeometry ? "cube" : "doc.text")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                if info.isSpatial {
                    Label("Spatial", systemImage: "square.dashed")
                        .font(.caption)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(.blue.opacity(0.1), in: Capsule())
                }
            }
        }
    }
}

struct PropertySetSection: View {
    let pset: PropertySet

    var body: some View {
        Section(pset.name) {
            ForEach(pset.properties, id: \.name) { prop in
                HStack(alignment: .top) {
                    Text(prop.name)
                        .frame(maxWidth: 160, alignment: .leading)
                    Spacer()
                    VStack(alignment: .trailing) {
                        Text(prop.value)
                            .foregroundStyle(.secondary)
                        if let unit = prop.unit {
                            Text(unit)
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
                .font(.callout)
            }
        }
    }
}

// MARK: - Section helper

struct Section<Content: View>: View {
    let title: String
    let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.headline)
                .foregroundStyle(.primary)
            content
            Divider()
        }
    }
}
