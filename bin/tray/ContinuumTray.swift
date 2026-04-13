import Cocoa

// ── continuum tray — native macOS thin client ───────────────
// Reads JSON from `continuum tray-data`. Renders native NSMenu.
// Zero business logic. The CLI is the brain. This is the eye.
//
// Build:  swiftc -O -o continuum-tray ContinuumTray.swift -framework Cocoa
// Size:   ~114KB

// ── JSON Model (matches `continuum tray-data` output) ────────

struct Services: Decodable {
    let healthy: Int
    let total: Int
}

struct TrayData: Decodable {
    let status: String          // green, yellow, red
    let statusText: String
    let docker: Bool
    let services: Services?
    let tailnet: String
    let nodes: [Node]
    let actions: [Action]
    let version: String

    struct Node: Decodable {
        let name: String
        let ip: String
        let online: Bool
        let isGrid: Bool
        let uiOk: Bool
        let url: String?
    }

    struct Action: Decodable {
        let id: String
        let label: String
        let command: String
    }
}

// ── App ──────────────────────────────────────────────────────

class ContinuumTray: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private var healthTimer: Timer?
    private var data = TrayData(
        status: "gray", statusText: "checking...", docker: false, services: nil,
        tailnet: "", nodes: [], actions: [], version: "1.0"
    )

    // Find CLI: installed location → PATH
    private lazy var cliBin: String = {
        let candidates = [
            NSHomeDirectory() + "/.local/bin/continuum",
            "/usr/local/bin/continuum",
            // Dev: repo relative to binary
            (ProcessInfo.processInfo.environment["CONTINUUM_HOME"] ?? NSHomeDirectory() + "/continuum") + "/bin/continuum",
        ]
        for p in candidates where FileManager.default.isExecutableFile(atPath: p) { return p }
        return "continuum"
    }()

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        renderIcon()
        renderMenu()
        refresh()
        healthTimer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            self?.refresh()
        }
    }

    // ── Refresh: call CLI, parse JSON, update UI ─────────────

    private var isRecovering = false

    private func refresh() {
        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self = self else { return }
            guard let json = self.runCLI("tray-data"),
                  let jsonData = json.data(using: .utf8),
                  let parsed = try? JSONDecoder().decode(TrayData.self, from: jsonData) else {
                DispatchQueue.main.async {
                    self.data = TrayData(
                        status: "red", statusText: "CLI unavailable", docker: false, services: nil,
                        tailnet: "", nodes: [], actions: [], version: "1.0"
                    )
                    self.renderIcon()
                    self.renderMenu()
                }
                return
            }
            DispatchQueue.main.async {
                self.data = parsed
                self.renderIcon()
                self.renderMenu()
            }

            // Auto-recovery: fix what we can without user intervention
            if !self.isRecovering {
                self.autoRecover(parsed)
            }
        }
    }

    /// Automatically recover from known bad states.
    /// The tray has special powers — it's always running, even when Docker is down.
    private func autoRecover(_ state: TrayData) {
        isRecovering = true
        defer { isRecovering = false }

        // 1. Docker not running → start it
        if !state.docker {
            #if os(macOS)
            let _ = runCLI("start") // continuum start handles Docker launch
            #endif
            return
        }

        // 2. Docker running but no healthy services → start them
        if state.docker && (state.services?.healthy ?? 0) == 0 {
            let _ = runCLI("start")
            return
        }
    }

    // ── Icon: ring + colored center dot (HAL 9000) ───────────

    private func renderIcon() {
        let color: NSColor = {
            switch data.status {
            case "green":  return .systemGreen
            case "yellow": return .systemYellow
            case "red":    return .systemRed
            default:       return .systemGray
            }
        }()

        let size = NSSize(width: 22, height: 22)
        let image = NSImage(size: size, flipped: false) { rect in
            let cx = rect.midX, cy = rect.midY
            let isDark = NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            let ringColor: NSColor = isDark
                ? .white.withAlphaComponent(0.75)
                : .black.withAlphaComponent(0.75)

            // Ring
            ringColor.setStroke()
            let ring = NSBezierPath(ovalIn: rect.insetBy(dx: 2.5, dy: 2.5))
            ring.lineWidth = 1.5
            ring.stroke()

            // Glow
            color.withAlphaComponent(0.15).setFill()
            NSBezierPath(ovalIn: NSRect(x: cx - 6, y: cy - 6, width: 12, height: 12)).fill()

            // Dot
            color.setFill()
            NSBezierPath(ovalIn: NSRect(x: cx - 3.5, y: cy - 3.5, width: 7, height: 7)).fill()

            return true
        }
        image.isTemplate = false
        statusItem.button?.image = image
        statusItem.button?.toolTip = "continuum — \(data.statusText)"
    }

    // ── Menu: built entirely from JSON data ──────────────────

    private func renderMenu() {
        let menu = NSMenu()

        // Header
        addDisabled(menu, "continuum")
        let statusIcon = data.status == "green" ? "🟢" : data.status == "yellow" ? "🟡" : "🔴"
        addDisabled(menu, "\(statusIcon)  \(data.statusText)")
        menu.addItem(.separator())

        // Nodes (from JSON)
        if data.nodes.isEmpty {
            addDisabled(menu, "No grid nodes")
        } else {
            for node in data.nodes {
                let icon = node.uiOk ? "🟢" : node.online ? "🟡" : "🔴"
                let item = NSMenuItem(title: "\(icon)  \(node.name)", action: #selector(nodeClicked(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = node.url ?? (node.isGrid ? "http://\(node.ip):9003" : nil)
                item.isEnabled = node.online && (node.url != nil || node.isGrid)
                menu.addItem(item)
            }
        }
        menu.addItem(.separator())

        // Actions (from JSON — primary)
        let primary = ["start", "stop", "restart"]
        for action in data.actions where primary.contains(action.id) {
            let item = NSMenuItem(title: action.label, action: #selector(actionClicked(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = action.command
            menu.addItem(item)
        }
        menu.addItem(.separator())

        // Actions (from JSON — tools submenu)
        let tools = NSMenu()
        for action in data.actions where !primary.contains(action.id) {
            let item = NSMenuItem(title: action.label, action: #selector(actionClicked(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = action.command
            tools.addItem(item)
        }
        let toolsItem = NSMenuItem(title: "Tools", action: nil, keyEquivalent: "")
        toolsItem.submenu = tools
        menu.addItem(toolsItem)
        menu.addItem(.separator())

        // Footer
        addDisabled(menu, "continuum v\(data.version)")
        let quit = NSMenuItem(title: "Quit continuum", action: #selector(quitApp), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)

        statusItem.menu = menu
    }

    private func addDisabled(_ menu: NSMenu, _ title: String) {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        menu.addItem(item)
    }

    // ── Actions ──────────────────────────────────────────────

    @objc private func nodeClicked(_ sender: NSMenuItem) {
        guard let urlString = sender.representedObject as? String,
              let url = URL(string: urlString) else { return }
        NSWorkspace.shared.open(url)
    }

    @objc private func actionClicked(_ sender: NSMenuItem) {
        guard let command = sender.representedObject as? String else { return }

        // Run via login shell so PATH includes ~/.local/bin where continuum lives
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/bin/bash")
            process.arguments = ["-l", "-c", command]
            process.standardOutput = FileHandle.nullDevice
            process.standardError = FileHandle.nullDevice
            try? process.run()
            process.waitUntilExit()

            DispatchQueue.main.asyncAfter(deadline: .now() + 5) {
                self?.refresh()
            }
        }
    }

    @objc private func quitApp() { NSApp.terminate(nil) }

    // ── CLI runner ───────────────────────────────────────────

    private func runCLI(_ command: String) -> String? {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = ["-l", "-c", "\"\(cliBin)\" \(command) 2>/dev/null"]
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            process.waitUntilExit()
            guard process.terminationStatus == 0 else { return nil }
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            return String(data: data, encoding: .utf8)
        } catch { return nil }
    }
}

// ── Main ─────────────────────────────────────────────────────
let app = NSApplication.shared
let delegate = ContinuumTray()
app.delegate = delegate
app.run()
