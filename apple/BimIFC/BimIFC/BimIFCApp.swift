import SwiftUI

@main
struct BimIFCApp: App {
    @StateObject private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(appState)
        }
        #if os(macOS)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Open IFC File...") {
                    appState.showFilePicker = true
                }
                .keyboardShortcut("o")
            }
        }
        #endif
    }
}
