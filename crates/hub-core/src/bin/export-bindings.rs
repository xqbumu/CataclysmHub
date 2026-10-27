use hub_core::{Dashboard, HubError, LaunchPlan, WorkspaceInput};
use ts_rs::{Config, TS};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/bindings");
    let cfg = Config::new().with_out_dir(directory);
    Dashboard::export_all(&cfg)?;
    LaunchPlan::export_all(&cfg)?;
    WorkspaceInput::export_all(&cfg)?;
    HubError::export_all(&cfg)?;
    Ok(())
}
