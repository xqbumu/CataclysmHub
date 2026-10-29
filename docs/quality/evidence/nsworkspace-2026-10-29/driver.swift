import AppKit
let root = URL(fileURLWithPath: CommandLine.arguments[1])
var results: [[String: Any]] = []
for mode in ["native", "script"] {
 let output = root.appendingPathComponent(mode + "-receiver.json")
 let configuration = NSWorkspace.OpenConfiguration()
 configuration.createsNewApplicationInstance = true
 configuration.activates = false
 configuration.arguments = ["--userdir", root.appendingPathComponent(mode + "-instance").path, "--savedir", root.appendingPathComponent(mode + "-instance/save").path]
 configuration.environment = ["HUB_PROBE_OUTPUT": output.path, "HUB_PROBE_MARKER": mode]
 var completed = false
 var application: NSRunningApplication?
 var row: [String: Any] = ["mode": mode]
 NSWorkspace.shared.openApplication(at: root.appendingPathComponent(mode + ".app"), configuration: configuration) { app, error in
  application = app
  row["callback_pid"] = app?.processIdentifier ?? -1
  row["callback_executable"] = app?.executableURL?.path ?? "missing"
  row["error"] = error?.localizedDescription ?? ""
  completed = true
 }
 let deadline = Date().addingTimeInterval(15)
 while !completed && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
 row["callback_completed"] = completed
 while let app = application, !app.isTerminated && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
 row["observed_termination"] = application?.isTerminated ?? false
 if let app = application, !app.isTerminated { row["timeout_terminate_requested"] = app.terminate() }
 if let data = try? Data(contentsOf: output), let receiver = try? JSONSerialization.jsonObject(with: data) { row["receiver"] = receiver }
 results.append(row)
}
let data = try JSONSerialization.data(withJSONObject: results, options: [.prettyPrinted, .sortedKeys])
try data.write(to: root.appendingPathComponent("result.json"))
print(String(data: data, encoding: .utf8)!)
