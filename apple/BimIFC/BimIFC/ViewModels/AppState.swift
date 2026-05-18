import SwiftUI
import Combine

/// Main application state — wraps the Rust IfcModel via UniFFI
@MainActor
class AppState: ObservableObject {
    @Published var model: IfcModel?
    @Published var fileName: String?
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var showFilePicker = false

    /// Incremented on every file load — used to trigger 3D reload reliably
    @Published var modelVersion: Int = 0

    // Selection
    @Published var selectedEntityId: UInt32?

    // Spatial tree (cached)
    @Published var spatialTree: SpatialNode?
    @Published var storeys: [StoreyInfo] = []
    @Published var metadata: ModelMetadata?

    // Light fixtures (cached)
    @Published var lightFixtures: [LightFixtureInfo] = []

    // Search
    @Published var searchText = ""
    @Published var searchResults: [UInt32] = []

    func openFile(url: URL) {
        isLoading = true
        errorMessage = nil
        fileName = url.lastPathComponent

        // Clear previous model state immediately
        selectedEntityId = nil
        searchText = ""
        searchResults = []

        // Parse on background thread — Rust does all the work
        let path = url.path
        Task.detached { [weak self] in
            do {
                let model = try IfcModel.fromFile(path: path)
                let tree = model.spatialTree()
                let storeys = model.storeys()
                let meta = model.metadata()
                let lights = model.lightFixtures()

                await MainActor.run {
                    self?.model = model
                    self?.spatialTree = tree
                    self?.storeys = storeys
                    self?.metadata = meta
                    self?.lightFixtures = lights
                    self?.isLoading = false
                    self?.modelVersion += 1
                }
            } catch {
                await MainActor.run {
                    self?.errorMessage = error.localizedDescription
                    self?.isLoading = false
                }
            }
        }
    }

    func search() {
        guard let model, !searchText.isEmpty else {
            searchResults = []
            return
        }
        searchResults = model.search(query: searchText)
    }

    func entityName(id: UInt32) -> String {
        model?.entityName(id: id) ?? "#\(id)"
    }

    func entityInfo(id: UInt32) -> EntityInfo? {
        model?.getEntity(id: id)
    }

    func propertySets(for id: UInt32) -> [PropertySet] {
        model?.propertySets(id: id) ?? []
    }

    func quantities(for id: UInt32) -> [Quantity] {
        model?.quantities(id: id) ?? []
    }
}
