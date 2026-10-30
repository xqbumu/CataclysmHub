import AppKit
let cfg = NSWorkspace.OpenConfiguration()
cfg.createsNewApplicationInstance = true
cfg.activates = true
cfg.arguments = Array(CommandLine.arguments.dropFirst(2))
var done = false
NSWorkspace.shared.openApplication(at: URL(fileURLWithPath: CommandLine.arguments[1]), configuration: cfg) { app, error in
 print("pid=\(app?.processIdentifier ?? -1) executable=\(app?.executableURL?.path ?? "none") error=\(String(describing: error))")
 done = true
}
let deadline = Date().addingTimeInterval(20)
while !done && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) }
print("callback=\(done)")
