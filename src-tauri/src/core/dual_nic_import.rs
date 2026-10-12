use crate::{
    config::{Config, IProfiles, IVerge},
    utils::dirs,
};
use anyhow::{Context as _, Result, ensure};
use clash_verge_draft::DraftTransaction;
use serde_yaml_ng::{Mapping, Value};
use std::path::{Component, Path, PathBuf};

fn strip_managed_script(text: &str) -> Result<String> {
    let mut out = text.to_owned();
    for (begin, end) in [
        ("// <ClashDualNic:managed:v1>", "// </ClashDualNic:managed:v1>"),
        ("// <CLASH-DUAL-NIC-MANAGED>", "// </CLASH-DUAL-NIC-MANAGED>"),
    ] {
        while let Some(start) = out.find(begin) {
            let tail = &out[start..];
            let finish = tail
                .find(end)
                .ok_or_else(|| anyhow::anyhow!("Incomplete managed policy block in imported script"))?
                + start
                + end.len();
            out.replace_range(start..finish, "");
        }
    }
    ensure!(
        !out.contains("// </ClashDualNic:managed:v1>") && !out.contains("// </CLASH-DUAL-NIC-MANAGED>"),
        "Orphan managed policy marker in imported script"
    );
    Ok(out)
}
fn preferences(source: Mapping) -> Result<IVerge> {
    let allowed = [
        "language",
        "theme_mode",
        "theme_setting",
        "start_page",
        "traffic_graph",
        "enable_memory_usage",
        "enable_group_icon",
        "collapse_navbar",
        "proxy_layout_column",
        "default_latency_test",
        "default_latency_timeout",
        "enable_dns_settings",
        "profile_dns_settings",
    ];
    let filtered: Mapping = source
        .into_iter()
        .filter(|(k, _)| k.as_str().is_some_and(|k| allowed.contains(&k)))
        .collect();
    Ok(serde_yaml_ng::from_value(Value::Mapping(filtered))?)
}
async fn optional_mapping(path: PathBuf) -> Result<Mapping> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => Ok(serde_yaml_ng::from_str(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Mapping::new()),
        Err(e) => Err(e.into()),
    }
}
const FILES: &[&str] = &[
    "profiles.yaml",
    "profiles",
    "verge.yaml",
    "config.yaml",
    "dns_config.yaml",
    "Country.mmdb",
    "ASN.mmdb",
    "geosite.dat",
    "geoip.dat",
];

// All paths are direct children of our own data directory. Partial backup moves also roll back.
async fn replace_files(destination: &Path, staged: &Path, backup: &Path) -> Result<()> {
    tokio::fs::create_dir_all(backup).await?;
    let mut moved = Vec::new();
    let mut installed = Vec::new();
    let result: Result<()> = async {
        for name in FILES {
            if !tokio::fs::try_exists(staged.join(name)).await? {
                continue;
            }
            if tokio::fs::try_exists(destination.join(name)).await? {
                tokio::fs::rename(destination.join(name), backup.join(name)).await?;
                moved.push(*name);
            }
            tokio::fs::rename(staged.join(name), destination.join(name)).await?;
            installed.push(*name);
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        let mut errors = Vec::new();
        for name in installed.into_iter().rev() {
            let target = destination.join(name);
            let removed = if name == "profiles" {
                tokio::fs::remove_dir_all(target).await
            } else {
                tokio::fs::remove_file(target).await
            };
            if let Err(e) = removed {
                errors.push(e.to_string());
            }
        }
        for name in moved.into_iter().rev() {
            if let Err(e) = tokio::fs::rename(backup.join(name), destination.join(name)).await {
                errors.push(e.to_string());
            }
        }
        ensure!(
            errors.is_empty(),
            "Import failed: {error:#}; rollback failed: {}. Backup: {}",
            errors.join("; "),
            backup.display()
        );
        return Err(error);
    }
    Ok(())
}

pub async fn import() -> Result<usize> {
    let _write = Config::lock_config_write().await;
    ensure!(
        !super::dual_nic::enabled().await,
        "Disable dual NIC before importing profiles"
    );
    let destination = dirs::app_home_dir()?;
    let source = ::dirs::data_dir()
        .context("Missing application data directory")?
        .join("io.github.clash-verge-rev.clash-verge-rev");
    let catalog = tokio::fs::read_to_string(source.join("profiles.yaml")).await?;
    let profiles: IProfiles = serde_yaml_ng::from_str(&catalog)?;
    let count = profiles.items.as_ref().map_or(0, |items| {
        items
            .iter()
            .filter(|p| matches!(p.itype.as_deref(), Some("remote" | "local")))
            .count()
    });
    ensure!(count > 0, "The official client contains no subscription profiles");
    for item in profiles.items.as_ref().into_iter().flatten() {
        if let Some(file) = &item.file {
            ensure!(
                Path::new(file.as_str())
                    .components()
                    .all(|c| matches!(c, Component::Normal(_))),
                "Imported profile paths must stay inside the copied profiles directory"
            );
        }
    }
    let profile_draft = Config::profiles().await;
    ensure!(
        !profile_draft.data_arc().items.as_ref().is_some_and(|items| items
            .iter()
            .any(|p| matches!(p.itype.as_deref(), Some("remote" | "local")))),
        "Import is available only before adding subscriptions to the custom client"
    );
    let verge = Config::verge().await;
    let clash = Config::clash().await;
    let transaction = DraftTransaction::begin(vec![&profile_draft, &verge, &clash])?;
    let preferences = preferences(optional_mapping(source.join("verge.yaml")).await?)?;
    let original_clash = optional_mapping(source.join("config.yaml")).await?;
    verge.edit_draft(|v| v.patch_config(&preferences));
    clash.edit_draft(|c| {
        for key in ["ipv6", "mode", "log-level"] {
            if let Some(value) = original_clash.get(key) {
                c.0.insert(key.into(), value.clone());
            }
        }
    });
    profile_draft.edit_draft(|p| *p = profiles);
    let staged = destination.join(format!(".dualnic-import-{}", nanoid::nanoid!()));
    tokio::fs::create_dir_all(staged.join("profiles")).await?;
    let result: Result<usize> = async {
        copy_profiles(&source.join("profiles"), &staged.join("profiles")).await?;
        tokio::fs::write(
            staged.join("profiles.yaml"),
            serde_yaml_ng::to_string(&**profile_draft.latest_arc())?,
        )
        .await?;
        tokio::fs::write(
            staged.join("verge.yaml"),
            serde_yaml_ng::to_string(&**verge.latest_arc())?,
        )
        .await?;
        tokio::fs::write(
            staged.join("config.yaml"),
            serde_yaml_ng::to_string(&clash.latest_arc().0)?,
        )
        .await?;
        for name in [
            "Country.mmdb",
            "ASN.mmdb",
            "geosite.dat",
            "geoip.dat",
            "dns_config.yaml",
        ] {
            if tokio::fs::try_exists(source.join(name)).await? {
                tokio::fs::copy(source.join(name), staged.join(name)).await?;
            }
        }
        let backup = destination.join(format!("dualnic-before-import-{}", nanoid::nanoid!()));
        replace_files(&destination, &staged, &backup).await?;
        Ok(count)
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&staged).await;
    let count = result?;
    transaction.commit();
    crate::core::notify::announce(crate::core::notify::Refresh::Profiles);
    crate::core::notify::announce(crate::core::notify::Refresh::Verge);
    Ok(count)
}
async fn copy_profiles(source: &Path, destination: &Path) -> Result<()> {
    let mut pending: Vec<(PathBuf, PathBuf)> = vec![(source.to_owned(), destination.to_owned())];
    while let Some((src, dst)) = pending.pop() {
        tokio::fs::create_dir_all(&dst).await?;
        let mut entries = tokio::fs::read_dir(src).await?;
        while let Some(entry) = entries.next_entry().await? {
            let kind = entry.file_type().await?;
            ensure!(!kind.is_symlink(), "Symbolic links cannot be imported as profile files");
            let target = dst.join(entry.file_name());
            if kind.is_dir() {
                pending.push((entry.path(), target));
            } else if kind.is_file() {
                if entry.path().extension().is_some_and(|e| e == "js") {
                    let text = tokio::fs::read_to_string(entry.path()).await?;
                    tokio::fs::write(target, strip_managed_script(&text)?).await?;
                } else {
                    tokio::fs::copy(entry.path(), target).await?;
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_only_the_old_helper_block() {
        let script =
            "function main(c) { return c; }\n// <ClashDualNic:managed:v1>\nmanaged();\n// </ClashDualNic:managed:v1>";
        assert_eq!(
            strip_managed_script(script).unwrap(),
            "function main(c) { return c; }\n"
        );
        assert!(strip_managed_script("// <ClashDualNic:managed:v1>").is_err());
        assert!(strip_managed_script("// </ClashDualNic:managed:v1>").is_err());
    }
    #[test]
    fn import_does_not_copy_service_or_startup_state() {
        let p = preferences(serde_yaml_ng::from_str("language: zh\ntheme_mode: dark").unwrap_or_default()).unwrap();
        assert!(p.enable_auto_launch.is_none());
        let p = preferences(serde_yaml_ng::from_str("language: zh\nenable_auto_launch: true\nauto_check_update: true\ndual_nic: {enabled: true}\nenable_system_proxy: true").unwrap()).unwrap();
        assert_eq!(p.language.as_deref(), Some("zh"));
        assert!(p.auto_check_update.is_none() && p.dual_nic.is_none() && p.enable_system_proxy.is_none());
    }
    #[tokio::test]
    async fn partial_import_backup_is_restored() -> Result<()> {
        let base = std::env::temp_dir().join(format!("dualnic-import-test-{}", nanoid::nanoid!()));
        let dest = base.join("destination");
        let staged = base.join("staged");
        let backup = base.join("backup");
        tokio::fs::create_dir_all(&dest).await?;
        tokio::fs::create_dir_all(&staged).await?;
        tokio::fs::write(dest.join("profiles.yaml"), "old").await?;
        tokio::fs::write(staged.join("profiles.yaml"), "new").await?;
        tokio::fs::write(dest.join("profiles"), "old-directory-conflict").await?;
        tokio::fs::create_dir_all(staged.join("profiles")).await?;
        tokio::fs::create_dir_all(&backup).await?;
        tokio::fs::create_dir_all(backup.join("profiles")).await?;
        tokio::fs::write(backup.join("profiles/occupied"), "force-backup-failure").await?;
        assert!(replace_files(&dest, &staged, &backup).await.is_err());
        assert_eq!(tokio::fs::read_to_string(dest.join("profiles.yaml")).await?, "old");
        assert_eq!(
            tokio::fs::read_to_string(dest.join("profiles")).await?,
            "old-directory-conflict"
        );
        tokio::fs::remove_dir_all(base).await?;
        Ok(())
    }
}
