fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        eprintln!("Usage: verify-restored <backup-directory> <recovery-container-directory>");
        eprintln!(
            "Read-only byte verification; reports do not resolve the original recovery task."
        );
        std::process::exit(2);
    }
    match hub_core::verify_restored_directory(
        std::path::Path::new(&args[0]),
        std::path::Path::new(&args[1]),
    ) {
        Ok(result) => match serde_json::to_string_pretty(&result) {
            Ok(json) => println!("{json}"),
            Err(_) => {
                eprintln!("SERIALIZATION_FAILED");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
