fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let output = if args.len() == 2 && args[0] == "--verify" {
        hub_core::verify_installation_copy(std::path::Path::new(&args[1])).and_then(|v| {
            serde_json::to_string_pretty(&v)
                .map_err(|_| hub_core::HubError::new("SERIALIZATION_FAILED", "无法输出结果。"))
        })
    } else if args.len() == 4 && (args[3] == "--preview" || args[3] == "--confirm-new-directory") {
        let executable = match args[1].to_str() {
            Some(v) => v,
            None => {
                eprintln!("PATH_UNSUPPORTED");
                std::process::exit(2)
            }
        };
        let source = std::path::Path::new(&args[0]);
        let destination = std::path::Path::new(&args[2]);
        if args[3] == "--preview" {
            hub_core::plan_installation_copy(source, executable, destination).and_then(|v| {
                serde_json::to_string_pretty(&v)
                    .map_err(|_| hub_core::HubError::new("SERIALIZATION_FAILED", "无法输出预览。"))
            })
        } else {
            hub_core::copy_installation_to_new_directory(source, executable, destination).and_then(
                |v| {
                    serde_json::to_string_pretty(&v).map_err(|_| {
                        hub_core::HubError::new("SERIALIZATION_FAILED", "无法输出结果。")
                    })
                },
            )
        }
    } else {
        eprintln!(
            "Usage: copy-installation <source-root> <relative-executable> <new-container> --preview|--confirm-new-directory\n       copy-installation --verify <container>"
        );
        std::process::exit(2)
    };
    match output {
        Ok(v) => println!("{v}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1)
        }
    }
}
