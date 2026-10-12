use crate::config::{Config, IVerge};
use anyhow::Result;
use clash_verge_draft::SharedDraft;
use serde_yaml_ng::Mapping;
use tokio::sync::MutexGuard;

pub async fn patch_clash(patch: &Mapping) -> Result<()> {
    let config_write = Config::try_lock_config_write()?;
    if crate::core::dual_nic::enabled().await {
        anyhow::ensure!(
            !patch.contains_key("dns") && !patch.contains_key("tun") && !patch.contains_key("mode"),
            "DNS, TUN and rule mode are managed by dual NIC policy"
        );
    }
    super::executor::apply(
        &config_write,
        super::executor::Patch::Clash(patch),
        super::effects::clash_effects(patch),
    )
    .await
}

/// Apply a patch, then reconcile TUN when its setting changes.
///
/// TUN patches do not always produce a Run State transition, so reconciliation is explicit.
pub async fn patch_verge(patch: &IVerge, not_save_file: bool) -> Result<()> {
    let config_write = Config::try_lock_config_write()?;
    anyhow::ensure!(patch.dual_nic.is_none(), "Use the dual NIC settings command");
    anyhow::ensure!(
        patch.auto_check_update != Some(true),
        "This custom client uses manual updates"
    );
    if crate::core::dual_nic::enabled().await {
        anyhow::ensure!(
            patch.enable_tun_mode != Some(false),
            "TUN is managed by dual NIC policy"
        );
    }
    apply_verge_patch_locked(&config_write, patch, not_save_file).await?;
    drop(config_write);
    if patch.enable_tun_mode.is_some() {
        super::reconcile_tun_availability().await;
    }
    Ok(())
}

/// Apply a patch without post-update reconciliation.
pub(super) async fn apply_verge_patch(patch: &IVerge, not_save_file: bool) -> Result<()> {
    let config_write = Config::try_lock_config_write()?;
    apply_verge_patch_locked(&config_write, patch, not_save_file).await
}

/// Apply a patch with the shared configuration write lock already held.
/// Callers must pass the guard returned by [`Config::lock_config_write`].
pub(crate) async fn apply_verge_patch_locked(
    _config_write: &MutexGuard<'_, ()>,
    patch: &IVerge,
    not_save_file: bool,
) -> Result<()> {
    super::executor::apply(
        _config_write,
        super::executor::Patch::Verge {
            patch,
            persist: !not_save_file,
        },
        super::effects::verge_effects(patch),
    )
    .await
}

pub async fn fetch_verge_config() -> Result<SharedDraft<IVerge>> {
    let draft = Config::verge().await;
    let data = draft.data_arc();
    Ok(data)
}
