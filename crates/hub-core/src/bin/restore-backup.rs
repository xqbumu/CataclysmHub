fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 || args[2] != "--confirm-new-directory" {
        eprintln!(
            "Usage: restore-backup <backup-directory> <new-container-directory> --confirm-new-directory"
        );
        eprintln!(
            "Creates new-container-directory/data. Existing targets are refused; failures retain incomplete data."
        );
        std::process::exit(2);
    }
    match hub_core::restore_backup_to_new_directory(
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
