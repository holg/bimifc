import SwiftUI
import SceneKit

/// 3D viewport that renders IFC geometry using SceneKit.
/// Mesh data comes from Rust (bimifc-ffi) — SceneKit only does rendering.
struct Viewport3DView: View {
    @EnvironmentObject var appState: AppState
    @StateObject private var viewModel = Viewport3DViewModel()

    var body: some View {
        SceneView(
            scene: viewModel.scene,
            pointOfView: viewModel.cameraNode,
            options: [.allowsCameraControl, .autoenablesDefaultLighting]
        )
        .overlay(alignment: .topTrailing) {
            VStack(spacing: 8) {
                Button {
                    viewModel.fitAll()
                } label: {
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                        .padding(8)
                        .background(.ultraThinMaterial, in: Circle())
                }
                Button {
                    viewModel.resetCamera()
                } label: {
                    Image(systemName: "house")
                        .padding(8)
                        .background(.ultraThinMaterial, in: Circle())
                }
                if viewModel.lightCount > 0 {
                    Button {
                        viewModel.toggleLights()
                    } label: {
                        Image(systemName: viewModel.lightsEnabled ? "lightbulb.fill" : "lightbulb.slash")
                            .padding(8)
                            .background(.ultraThinMaterial, in: Circle())
                            .foregroundStyle(viewModel.lightsEnabled ? .yellow : .secondary)
                    }
                }
            }
            .padding(12)
        }
        .overlay(alignment: .bottomLeading) {
            if viewModel.meshCount > 0 {
                HStack(spacing: 12) {
                    Text("\(viewModel.meshCount) meshes, \(viewModel.triangleCount) tris")
                    if viewModel.lightCount > 0 {
                        Text("\(viewModel.lightCount) lights")
                            .foregroundStyle(.yellow)
                    }
                }
                .font(.caption)
                .padding(6)
                .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 6))
                .padding(8)
            }
        }
        .onChange(of: appState.modelVersion) {
            loadAllGeometry()
        }
        .onChange(of: appState.selectedEntityId) {
            if let id = appState.selectedEntityId {
                viewModel.highlight(entityId: id)
            } else {
                viewModel.clearHighlight()
            }
        }
        .onAppear {
            if appState.model != nil {
                loadAllGeometry()
            }
        }
    }

    private func loadAllGeometry() {
        guard let model = appState.model else { return }
        let lights = appState.lightFixtures
        Task.detached {
            // Use allGeometryIds — scans all entities for geometry,
            // not just those in the spatial tree (which may be incomplete)
            let elementIds = model.allGeometryIds()
            let meshes = model.batchGeometry(ids: elementIds)

            // Also try all_elements as fallback
            var allMeshes = meshes
            if meshes.isEmpty {
                let spatialIds = model.allElements()
                allMeshes = model.batchGeometry(ids: spatialIds)
            }

            // Also try all entity IDs as last resort
            if allMeshes.isEmpty {
                let allIds = model.allEntityIds()
                allMeshes = model.batchGeometry(ids: allIds)
            }

            await MainActor.run {
                viewModel.loadMeshes(allMeshes, lightFixtures: lights)
            }
        }
    }
}

// MARK: - ViewModel

@MainActor
class Viewport3DViewModel: ObservableObject {
    let scene = SCNScene()
    let cameraNode = SCNNode()

    @Published var meshCount = 0
    @Published var triangleCount = 0
    @Published var lightCount = 0
    @Published var lightsEnabled = true

    private let geometryRoot = SCNNode()
    private let lightsRoot = SCNNode()
    private var entityNodes: [UInt32: SCNNode] = [:]
    private var highlightedEntityId: UInt32?
    private var originalMaterials: [UInt32: [SCNMaterial]] = [:]

    init() {
        setupScene()
    }

    private func setupScene() {
        cameraNode.camera = SCNCamera()
        cameraNode.camera?.zNear = 0.01
        cameraNode.camera?.zFar = 100000
        cameraNode.camera?.automaticallyAdjustsZRange = true
        cameraNode.position = SCNVector3(20, 20, 20)
        cameraNode.look(at: SCNVector3Zero)
        scene.rootNode.addChildNode(cameraNode)

        // Ambient light
        let ambientLight = SCNNode()
        ambientLight.light = SCNLight()
        ambientLight.light?.type = .ambient
        ambientLight.light?.intensity = 300
        ambientLight.light?.color = NSColor.white
        scene.rootNode.addChildNode(ambientLight)

        // Directional light
        let directional = SCNNode()
        directional.light = SCNLight()
        directional.light?.type = .directional
        directional.light?.intensity = 600
        directional.light?.castsShadow = true
        directional.position = SCNVector3(50, 100, 50)
        directional.look(at: SCNVector3Zero)
        scene.rootNode.addChildNode(directional)

        // Second directional (fill)
        let fill = SCNNode()
        fill.light = SCNLight()
        fill.light?.type = .directional
        fill.light?.intensity = 300
        fill.position = SCNVector3(-30, 60, -40)
        fill.look(at: SCNVector3Zero)
        scene.rootNode.addChildNode(fill)

        scene.background.contents = NSColor(red: 0.12, green: 0.13, blue: 0.16, alpha: 1.0)

        // Floor
        let floor = SCNFloor()
        floor.reflectivity = 0.05
        let floorMat = SCNMaterial()
        floorMat.diffuse.contents = NSColor(white: 0.2, alpha: 1.0)
        floor.materials = [floorMat]
        scene.rootNode.addChildNode(SCNNode(geometry: floor))

        scene.rootNode.addChildNode(geometryRoot)
        scene.rootNode.addChildNode(lightsRoot)
    }

    func loadMeshes(_ meshes: [EntityMesh], lightFixtures: [LightFixtureInfo]) {
        // Clear everything
        geometryRoot.childNodes.forEach { $0.removeFromParentNode() }
        lightsRoot.childNodes.forEach { $0.removeFromParentNode() }
        entityNodes.removeAll()
        originalMaterials.removeAll()
        highlightedEntityId = nil

        var totalTriangles = 0

        NSLog("[Viewport] Received %d entity meshes from Rust", meshes.count)
        for entityMesh in meshes {
            let mesh = entityMesh.mesh
            NSLog("[Viewport]   #%d %@ verts=%d tris=%d posLen=%d",
                  entityMesh.entityId, entityMesh.ifcType,
                  mesh.vertexCount, mesh.triangleCount, mesh.positions.count)
            guard mesh.vertexCount > 0 && mesh.triangleCount > 0 else { continue }

            let node = createNode(from: mesh, ifcType: entityMesh.ifcType, entityId: entityMesh.entityId)
            node.name = "entity_\(entityMesh.entityId)"
            geometryRoot.addChildNode(node)
            entityNodes[entityMesh.entityId] = node
            totalTriangles += Int(mesh.triangleCount)
        }

        // Add SceneKit point lights for IfcLightFixture positions
        for fixture in lightFixtures {
            guard fixture.position.count == 3 else { continue }
            let lightNode = SCNNode()
            lightNode.light = SCNLight()
            lightNode.light?.type = .omni
            lightNode.light?.color = colorFromTemperature(fixture.colorTemperature ?? 4000)
            // Scale intensity: 1000 lm ≈ 200 SceneKit intensity
            let flux = fixture.luminousFlux ?? 1000
            lightNode.light?.intensity = CGFloat(flux * 0.2)
            lightNode.light?.attenuationStartDistance = 0.5
            lightNode.light?.attenuationEndDistance = CGFloat(Swift.max(5, flux * 0.01))
            lightNode.position = SCNVector3(
                fixture.position[0],
                fixture.position[1],
                fixture.position[2]
            )
            lightNode.name = "light_\(fixture.entityId)"

            // Small emissive sphere to visualize fixture position
            let indicator = SCNSphere(radius: 0.08)
            let mat = SCNMaterial()
            mat.diffuse.contents = NSColor.black
            mat.emission.contents = colorFromTemperature(fixture.colorTemperature ?? 4000)
            mat.lightingModel = .constant
            indicator.materials = [mat]
            let indicatorNode = SCNNode(geometry: indicator)
            lightNode.addChildNode(indicatorNode)

            lightsRoot.addChildNode(lightNode)
        }

        meshCount = entityNodes.count
        triangleCount = totalTriangles
        lightCount = lightFixtures.count
        lightsEnabled = true

        NSLog("[Viewport] Loaded %d meshes, %d triangles, %d lights", meshCount, triangleCount, lightCount)
        fitAll()
        NSLog("[Viewport] Camera at (%f, %f, %f) zFar=%f",
              cameraNode.position.x, cameraNode.position.y, cameraNode.position.z,
              cameraNode.camera?.zFar ?? 0)
    }

    func toggleLights() {
        lightsEnabled.toggle()
        lightsRoot.isHidden = !lightsEnabled
    }

    private func createNode(from mesh: MeshData, ifcType: String, entityId: UInt32) -> SCNNode {
        let vertexCount = Int(mesh.vertexCount)

        let posData = mesh.positions.withUnsafeBytes { Data($0) }
        let positionSource = SCNGeometrySource(
            data: posData,
            semantic: .vertex,
            vectorCount: vertexCount,
            usesFloatComponents: true,
            componentsPerVector: 3,
            bytesPerComponent: MemoryLayout<Float>.size,
            dataOffset: 0,
            dataStride: 3 * MemoryLayout<Float>.size
        )

        let normData = mesh.normals.withUnsafeBytes { Data($0) }
        let normalSource = SCNGeometrySource(
            data: normData,
            semantic: .normal,
            vectorCount: vertexCount,
            usesFloatComponents: true,
            componentsPerVector: 3,
            bytesPerComponent: MemoryLayout<Float>.size,
            dataOffset: 0,
            dataStride: 3 * MemoryLayout<Float>.size
        )

        let indexData = mesh.indices.withUnsafeBytes { Data($0) }
        let element = SCNGeometryElement(
            data: indexData,
            primitiveType: .triangles,
            primitiveCount: Int(mesh.triangleCount),
            bytesPerIndex: MemoryLayout<UInt32>.size
        )

        let geometry = SCNGeometry(sources: [positionSource, normalSource], elements: [element])

        let material = SCNMaterial()
        material.lightingModel = .physicallyBased
        let (color, alpha, roughness, metalness) = materialForType(ifcType)
        material.diffuse.contents = color
        material.transparency = CGFloat(alpha)
        material.roughness.contents = roughness
        material.metalness.contents = metalness
        material.isDoubleSided = true
        geometry.materials = [material]

        return SCNNode(geometry: geometry)
    }

    /// IFC type-based material assignment
    private func materialForType(_ ifcType: String) -> (NSColor, Float, Float, Float) {
        let upper = ifcType.uppercased()
        // (color, alpha, roughness, metalness)
        if upper.contains("WALL") {
            return (NSColor(red: 0.88, green: 0.87, blue: 0.82, alpha: 1), 1.0, 0.85, 0.0)
        } else if upper.contains("SLAB") {
            return (NSColor(red: 0.78, green: 0.76, blue: 0.72, alpha: 1), 1.0, 0.9, 0.0)
        } else if upper.contains("ROOF") {
            return (NSColor(red: 0.65, green: 0.42, blue: 0.32, alpha: 1), 1.0, 0.7, 0.05)
        } else if upper.contains("WINDOW") {
            return (NSColor(red: 0.65, green: 0.82, blue: 0.95, alpha: 1), 0.4, 0.1, 0.0)
        } else if upper.contains("DOOR") {
            return (NSColor(red: 0.55, green: 0.40, blue: 0.30, alpha: 1), 1.0, 0.6, 0.0)
        } else if upper.contains("BEAM") || upper.contains("COLUMN") || upper.contains("MEMBER") {
            return (NSColor(red: 0.62, green: 0.62, blue: 0.67, alpha: 1), 1.0, 0.4, 0.5)
        } else if upper.contains("STAIR") || upper.contains("RAMP") {
            return (NSColor(red: 0.72, green: 0.70, blue: 0.66, alpha: 1), 1.0, 0.8, 0.0)
        } else if upper.contains("RAILING") {
            return (NSColor(red: 0.50, green: 0.50, blue: 0.55, alpha: 1), 1.0, 0.3, 0.6)
        } else if upper.contains("PLATE") {
            return (NSColor(red: 0.72, green: 0.72, blue: 0.76, alpha: 1), 1.0, 0.4, 0.4)
        } else if upper.contains("COVERING") {
            return (NSColor(red: 0.90, green: 0.90, blue: 0.88, alpha: 1), 1.0, 0.9, 0.0)
        } else if upper.contains("FURNISHING") || upper.contains("FURNITURE") {
            return (NSColor(red: 0.62, green: 0.52, blue: 0.42, alpha: 1), 1.0, 0.7, 0.0)
        } else if upper.contains("LIGHTFIXTURE") {
            return (NSColor(red: 0.95, green: 0.92, blue: 0.75, alpha: 1), 1.0, 0.3, 0.2)
        } else if upper.contains("FLOW") || upper.contains("DISTRIBUTION") || upper.contains("PIPE") {
            return (NSColor(red: 0.45, green: 0.60, blue: 0.70, alpha: 1), 1.0, 0.5, 0.3)
        } else if upper.contains("SPACE") {
            return (NSColor(red: 0.70, green: 0.80, blue: 0.90, alpha: 1), 0.15, 0.9, 0.0)
        } else if upper.contains("FOOTING") || upper.contains("PILE") {
            return (NSColor(red: 0.60, green: 0.58, blue: 0.55, alpha: 1), 1.0, 0.95, 0.0)
        } else if upper.contains("PROXY") || upper.contains("BUILDINGELEMENTPROXY") {
            return (NSColor(red: 0.70, green: 0.70, blue: 0.72, alpha: 1), 1.0, 0.7, 0.1)
        } else {
            // Default gray
            return (NSColor(red: 0.68, green: 0.68, blue: 0.70, alpha: 1), 1.0, 0.7, 0.1)
        }
    }

    /// Convert color temperature (K) to approximate RGB
    private func colorFromTemperature(_ kelvin: Double) -> NSColor {
        let temp = kelvin / 100.0
        let r: Double
        let g: Double
        let b: Double

        if temp <= 66 {
            r = 1.0
            g = max(0, min(1, (99.4708025861 * log(temp) - 161.1195681661) / 255.0))
            if temp <= 19 {
                b = 0
            } else {
                b = max(0, min(1, (138.5177312231 * log(temp - 10) - 305.0447927307) / 255.0))
            }
        } else {
            r = max(0, min(1, (329.698727446 * pow(temp - 60, -0.1332047592)) / 255.0))
            g = max(0, min(1, (288.1221695283 * pow(temp - 60, -0.0755148492)) / 255.0))
            b = 1.0
        }
        return NSColor(red: r, green: g, blue: b, alpha: 1.0)
    }

    func highlight(entityId: UInt32) {
        clearHighlight()

        guard let node = entityNodes[entityId],
              let geometry = node.geometry else { return }

        highlightedEntityId = entityId
        originalMaterials[entityId] = geometry.materials

        let highlight = SCNMaterial()
        highlight.diffuse.contents = NSColor.systemBlue
        highlight.emission.contents = NSColor.systemBlue.withAlphaComponent(0.3)
        highlight.lightingModel = .physicallyBased
        highlight.roughness.contents = 0.5
        highlight.isDoubleSided = true
        geometry.materials = [highlight]
    }

    func clearHighlight() {
        if let id = highlightedEntityId,
           let node = entityNodes[id],
           let mats = originalMaterials[id] {
            node.geometry?.materials = mats
            originalMaterials.removeValue(forKey: id)
        }
        highlightedEntityId = nil
    }

    func fitAll() {
        let (min, max) = geometryRoot.boundingBox
        let center = SCNVector3(
            (min.x + max.x) / 2,
            (min.y + max.y) / 2,
            (min.z + max.z) / 2
        )
        let size = SCNVector3(
            max.x - min.x,
            max.y - min.y,
            max.z - min.z
        )
        let maxDim = Swift.max(size.x, Swift.max(size.y, size.z))
        guard maxDim > 0 else { return }

        let distance = maxDim * 1.5
        cameraNode.position = SCNVector3(
            center.x + distance * 0.5,
            center.y + distance * 0.7,
            center.z + distance * 0.5
        )
        cameraNode.look(at: center)
    }

    func resetCamera() {
        fitAll()
    }
}

// MARK: - Platform compat
#if os(iOS)
import UIKit
typealias NSColor = UIColor
#endif
