use hub_core::{Dashboard, HubError, LaunchPlan, ScanReport, WorkspaceInput};
use ts_rs::{Config, TS};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/bindings");
    let cfg = Config::new().with_out_dir(directory);
    Dashboard::export_all(&cfg)?;
    hub_core::PlayInstanceInput::export_all(&cfg)?;
    hub_core::RetentionPreview::export_all(&cfg)?;
    ScanReport::export_all(&cfg)?;
    hub_core::BackupPlan::export_all(&cfg)?;
    hub_core::BackupDetailsInput::export_all(&cfg)?;
    hub_core::RestorePlan::export_all(&cfg)?;
    LaunchPlan::export_all(&cfg)?;
    WorkspaceInput::export_all(&cfg)?;
    HubError::export_all(&cfg)?;
    Ok(())
}
