import AppKit
let bundle = URL(fileURLWithPath: CommandLine.arguments[1])
let data = URL(fileURLWithPath: CommandLine.arguments[2])
let config = NSWorkspace.OpenConfiguration()
config.createsNewApplicationInstance = true
config.activates = true
config.arguments = ["--version", "--userdir", data.path + "/", "--savedir", data.appendingPathComponent("save").path + "/", "--configdir", data.appendingPathComponent("config").path + "/"]
config.environment = ["HOME": data.path, "TMPDIR": data.path, "XDG_CONFIG_HOME": data.path, "XDG_CACHE_HOME": data.path, "XDG_DATA_HOME": data.path, "XDG_STATE_HOME": data.path]
var completed = false
NSWorkspace.shared.openApplication(at: bundle, configuration: config) { app, error in
 print("pid=\(app?.processIdentifier ?? -1) executable=\(app?.executableURL?.path ?? "none") error=\(String(describing: error))")
 completed = true
}
let deadline = Date().addingTimeInterval(45)
while !completed && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) }
print("callback_completed=\(completed)")
