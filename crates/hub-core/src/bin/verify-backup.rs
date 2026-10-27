fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("Usage: verify-backup <backup-directory>");
        std::process::exit(2);
    };
    match hub_core::verify_backup_directory(std::path::Path::new(&path)) {
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
