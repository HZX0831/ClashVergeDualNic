#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelIdentity {
    pub id: &'static str,
    pub service_slug: &'static str,
    pub windows_service_name: &'static str,
    pub service_display_name: &'static str,
    pub macos_app_bundle_id: &'static str,
    pub macos_service_id: &'static str,
}

#[cfg(all(not(feature = "development-channel"), not(feature = "dual-nic-channel")))]
pub const CHANNEL_IDENTITY: ChannelIdentity = ChannelIdentity {
    id: "production",
    service_slug: "clash-verge-service",
    windows_service_name: "clash_verge_service",
    service_display_name: "Clash Verge Service",
    macos_app_bundle_id: "io.github.clash-verge-rev.clash-verge-rev",
    macos_service_id: "io.github.clash-verge-rev.clash-verge-rev.service",
};

#[cfg(all(feature = "development-channel", not(feature = "dual-nic-channel")))]
pub const CHANNEL_IDENTITY: ChannelIdentity = ChannelIdentity {
    id: "development",
    service_slug: "clash-verge-service-dev",
    windows_service_name: "clash_verge_service_dev",
    service_display_name: "Clash Verge Development Service",
    macos_app_bundle_id: "io.github.clash-verge-rev.clash-verge-rev.dev",
    macos_service_id: "io.github.clash-verge-rev.clash-verge-rev.dev.service",
};

#[cfg(feature = "dual-nic-channel")]
pub const CHANNEL_IDENTITY: ChannelIdentity = ChannelIdentity {
    id: "dual-nic",
    service_slug: "clash-verge-service-dualnic",
    windows_service_name: "clash_verge_dualnic_service",
    service_display_name: "Clash Verge Dual NIC Service",
    macos_app_bundle_id: "io.github.clash-verge-dualnic",
    macos_service_id: "io.github.clash-verge-dualnic.service",
};

#[cfg(feature = "dual-nic-channel")]
pub const SERVICE_BINARY_STEM: &str = "clash-verge-dualnic-service";
#[cfg(not(feature = "dual-nic-channel"))]
pub const SERVICE_BINARY_STEM: &str = "clash-verge-service";

pub const SERVICE_SLUG: &str = CHANNEL_IDENTITY.service_slug;
pub const WINDOWS_SERVICE_NAME: &str = CHANNEL_IDENTITY.windows_service_name;
pub const SERVICE_DISPLAY_NAME: &str = CHANNEL_IDENTITY.service_display_name;
pub const MACOS_APP_BUNDLE_ID: &str = CHANNEL_IDENTITY.macos_app_bundle_id;
pub const MACOS_SERVICE_ID: &str = CHANNEL_IDENTITY.macos_service_id;

#[cfg(test)]
mod tests {
    use super::CHANNEL_IDENTITY;
    #[cfg(feature = "dual-nic-channel")]
    use super::SERVICE_BINARY_STEM;

    #[cfg(all(windows, feature = "dual-nic-channel", not(feature = "test")))]
    #[test]
    fn dual_nic_pipe_and_owner_namespaces_are_isolated() {
        assert_eq!(crate::SERVICE_SLUG, "clash-verge-service-dualnic");
        assert_eq!(crate::IPC_PATH, r"\\.\pipe\clash-verge-dualnic-service");
        let owner = crate::OwnerIdentity::Windows {
            sid: "S-1-5-21-1-2-3-1001".into(),
        };
        assert_eq!(
            crate::mihomo_ipc_path(&owner),
            format!(r"\\.\pipe\verge-mihomo-dual-nic-{}", crate::owner_key(&owner))
        );
        assert_ne!(CHANNEL_IDENTITY.windows_service_name, "clash_verge_service");
    }

    #[test]
    fn compiled_channel_has_a_self_consistent_identity() {
        assert!(!CHANNEL_IDENTITY.id.is_empty());
        assert!(CHANNEL_IDENTITY.service_slug.starts_with("clash-verge-service"));
        assert!(CHANNEL_IDENTITY.windows_service_name.starts_with("clash_verge_"));
        #[cfg(feature = "dual-nic-channel")]
        {
            assert_eq!(SERVICE_BINARY_STEM, "clash-verge-dualnic-service");
            assert_eq!(CHANNEL_IDENTITY.windows_service_name, "clash_verge_dualnic_service");
            assert_eq!(CHANNEL_IDENTITY.service_slug, "clash-verge-service-dualnic");
            #[cfg(not(feature = "test"))]
            assert_eq!(crate::IPC_PATH, r"\\.\pipe\clash-verge-dualnic-service");
            assert_eq!(CHANNEL_IDENTITY.id, "dual-nic");
        }
        assert!(
            CHANNEL_IDENTITY
                .macos_service_id
                .starts_with(CHANNEL_IDENTITY.macos_app_bundle_id)
        );
    }
}
