import AppKit
let root = URL(fileURLWithPath: CommandLine.arguments[1])
var apps: [NSRunningApplication] = []
var rows: [[String: Any]] = []
for index in 1...2 {
 let mode = "native-\(index)"
 let config = NSWorkspace.OpenConfiguration()
 config.createsNewApplicationInstance = true
 config.activates = false
 config.arguments = ["--userdir", root.appendingPathComponent(mode + "-instance").path]
 config.environment = ["HUB_PROBE_OUTPUT": root.appendingPathComponent(mode + "-receiver.json").path, "HUB_PROBE_MARKER": mode]
 var complete = false
 NSWorkspace.shared.openApplication(at: root.appendingPathComponent("native.app"), configuration: config) { app, error in
  rows.append(["mode": mode, "callback_pid": app?.processIdentifier ?? -1, "error": error?.localizedDescription ?? ""])
  if let app = app { apps.append(app) }
  complete = true
 }
 let deadline = Date().addingTimeInterval(10)
 while !complete && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
}
let simultaneous = apps.count == 2 && apps.allSatisfy { !$0.isTerminated } && apps[0].processIdentifier != apps[1].processIdentifier
let deadline = Date().addingTimeInterval(10)
while apps.contains(where: { !$0.isTerminated }) && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
for index in rows.indices {
 let mode = rows[index]["mode"] as! String
 if let data = try? Data(contentsOf: root.appendingPathComponent(mode + "-receiver.json")), let receiver = try? JSONSerialization.jsonObject(with: data) { rows[index]["receiver"] = receiver }
}
let result: [String: Any] = ["simultaneous_distinct_processes": simultaneous, "all_terminated": apps.allSatisfy { $0.isTerminated }, "rows": rows]
let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
try data.write(to: root.appendingPathComponent("result-pair.json"))
print(String(data: data, encoding: .utf8)!)
for app in apps where !app.isTerminated { _ = app.terminate() }
