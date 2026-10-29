import AppKit
let root = URL(fileURLWithPath: CommandLine.arguments[1])
let instance = root.appendingPathComponent("game-preserved-instance")
let resources = root.appendingPathComponent("game-preserved.app/Contents/Resources")
let config = NSWorkspace.OpenConfiguration()
config.createsNewApplicationInstance = true
config.activates = true
config.arguments = ["--basepath", resources.path + "/", "--userdir", instance.path + "/", "--savedir", instance.appendingPathComponent("save").path + "/", "--configdir", instance.appendingPathComponent("config").path + "/"]
config.environment = ["HOME":instance.appendingPathComponent("home").path,"TMPDIR":instance.appendingPathComponent("tmp").path,"XDG_CONFIG_HOME":instance.appendingPathComponent("config").path,"XDG_CACHE_HOME":instance.appendingPathComponent("cache").path,"XDG_DATA_HOME":instance.appendingPathComponent("data").path,"XDG_STATE_HOME":instance.appendingPathComponent("state").path,"DYLD_FRAMEWORK_PATH":resources.path,"DYLD_LIBRARY_PATH":resources.path]
var complete = false
var result: [String: Any] = ["arguments": config.arguments, "environment":config.environment]
NSWorkspace.shared.openApplication(at: root.appendingPathComponent("game-preserved.app"), configuration:config) { app,error in
 result["callback_pid"] = app?.processIdentifier ?? -1
 result["bundle_url"] = app?.bundleURL?.path ?? "missing"
 result["executable_url"] = app?.executableURL?.path ?? "missing"
 result["terminated_at_callback"] = app?.isTerminated ?? true
 result["error"] = error?.localizedDescription ?? ""
 complete = true
}
let deadline=Date().addingTimeInterval(25)
while !complete && Date()<deadline { RunLoop.current.run(until:Date().addingTimeInterval(0.05)) }
result["callback_completed"]=complete
let data=try JSONSerialization.data(withJSONObject:result,options:[.prettyPrinted,.sortedKeys]);try data.write(to:root.appendingPathComponent("result-game-preserved.json"));print(String(data:data,encoding:.utf8)!)
