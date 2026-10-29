import AppKit
import Darwin
let environment = ProcessInfo.processInfo.environment
let record: [String: Any] = ["pid": getpid(), "ppid": getppid(), "argv": Array(CommandLine.arguments.dropFirst()), "marker": environment["HUB_PROBE_MARKER"] ?? "missing", "executable": CommandLine.arguments[0]]
let data = try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
try data.write(to: URL(fileURLWithPath: environment["HUB_PROBE_OUTPUT"]!))
let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
DispatchQueue.main.asyncAfter(deadline: .now() + 4) { exit(7) }
app.run()
