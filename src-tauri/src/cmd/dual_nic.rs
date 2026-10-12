use super::{CmdResult, StringifyErr as _};
use crate::core::dual_nic;
use clash_verge_dualnic::{Adapter, Settings};
#[tauri::command]
pub fn list_physical_adapters() -> CmdResult<Vec<Adapter>> {
    dual_nic::adapters().stringify_err()
}
#[tauri::command]
pub async fn preview_dual_nic(settings: Settings) -> CmdResult<dual_nic::Preview> {
    dual_nic::preview(settings).await.stringify_err()
}
#[tauri::command]
pub async fn apply_dual_nic(settings: Settings) -> CmdResult<dual_nic::Status> {
    dual_nic::apply(settings).await.stringify_err()
}
#[tauri::command]
pub async fn get_dual_nic_status() -> CmdResult<dual_nic::Status> {
    dual_nic::status().await.stringify_err()
}
#[tauri::command]
pub async fn test_dual_nic() -> CmdResult<dual_nic::Detection> {
    dual_nic::test().await.stringify_err()
}

#[tauri::command]
pub async fn import_official_profiles() -> CmdResult<usize> {
    crate::core::dual_nic_import::import().await.stringify_err()
}

#[tauri::command]
pub async fn get_adapter_traffic() -> CmdResult<dual_nic::TrafficSnapshot> {
    dual_nic::traffic_snapshot().await.stringify_err()
}
