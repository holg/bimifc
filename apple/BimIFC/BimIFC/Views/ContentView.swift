import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        NavigationSplitView {
            SidebarView()
        } detail: {
            if appState.model != nil {
                HSplitView {
                    // 3D viewport takes most of the space
                    Viewport3DView()
                        .frame(minWidth: 400)

                    // Properties panel on the right
                    if appState.selectedEntityId != nil {
                        DetailView()
                            .frame(minWidth: 280, idealWidth: 320, maxWidth: 400)
                    }
                }
            } else {
                WelcomeView()
            }
        }
        .fileImporter(
            isPresented: $appState.showFilePicker,
            allowedContentTypes: [UTType(filenameExtension: "ifc") ?? .data],
            allowsMultipleSelection: false
        ) { result in
            switch result {
            case .success(let urls):
                if let url = urls.first {
                    _ = url.startAccessingSecurityScopedResource()
                    appState.openFile(url: url)
                }
            case .failure(let error):
                appState.errorMessage = error.localizedDescription
            }
        }
        .overlay {
            if appState.isLoading {
                LoadingOverlay()
            }
        }
        .alert("Error", isPresented: .constant(appState.errorMessage != nil)) {
            Button("OK") { appState.errorMessage = nil }
        } message: {
            Text(appState.errorMessage ?? "")
        }
    }
}

struct LoadingOverlay: View {
    var body: some View {
        ZStack {
            Color.black.opacity(0.3)
                .ignoresSafeArea()
            VStack(spacing: 16) {
                ProgressView()
                    .scaleEffect(1.5)
                Text("Parsing IFC file...")
                    .font(.headline)
                    .foregroundStyle(.secondary)
            }
            .padding(32)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 16))
        }
    }
}
