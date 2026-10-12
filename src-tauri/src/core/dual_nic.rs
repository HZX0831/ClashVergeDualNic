use crate::{
    config::{Config, IVerge},
    core::{
        CoreManager,
        handle::Handle,
        notification::{FrontendEvent, NotificationSystem},
        validate::{CoreConfigValidator, ValidationOutcome},
    },
    utils::dirs,
};
use anyhow::{Result, ensure};
use clash_verge_dualnic::{Adapter, CN, Context, Report, Routes, Settings, UNMATCHED, USB};
use parking_lot::Mutex;
use serde::Serialize;
use serde_yaml_ng::Mapping;
use std::sync::{
    LazyLock,
    atomic::{AtomicU64, Ordering},
};

tokio::task_local! { static REPORT_CAPTURE: std::sync::Arc<Mutex<Option<Report>>>; }

static GENERATION: AtomicU64 = AtomicU64::new(0);
static LAST_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(Mutex::default);

pub async fn settings() -> Settings {
    Config::verge().await.latest_arc().dual_nic.clone().unwrap_or_default()
}
pub async fn enabled() -> bool {
    settings().await.enabled
}
pub fn adapters() -> Result<Vec<Adapter>> {
    #[cfg(windows)]
    {
        clash_verge_dualnic::windows::adapters()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Dual NIC is available on Windows only")
    }
}
pub fn ensure_exclusive() -> Result<()> {
    #[cfg(windows)]
    {
        let identity = crate::core::owner_identity::current_owner_identity()?;
        let pipe = format!(
            r"\\.\pipe\verge-mihomo-production-{}",
            clash_verge_service_ipc::owner_key(&identity)
        );
        ensure!(
            !clash_verge_dualnic::windows::official_client_running()?
                && !clash_verge_dualnic::windows::pipe_present(&pipe),
            "Exit the official Clash Verge client and its core before starting this custom client"
        );
    }
    Ok(())
}
pub async fn context(settings: &Settings) -> Result<Context> {
    let runtime = Config::runtime().await.latest_arc();
    let ipv6 = runtime
        .config
        .as_ref()
        .and_then(|c| c.get("ipv6"))
        .and_then(|v| v.as_bool());
    context_for(settings, ipv6).await
}
async fn context_for(settings: &Settings, ipv6: Option<bool>) -> Result<Context> {
    let mut inventory = tokio::task::spawn_blocking(adapters).await??;
    let gui_ipv6 = Config::clash()
        .await
        .latest_arc()
        .0
        .get("ipv6")
        .and_then(|v| v.as_bool());
    if ipv6.or(gui_ipv6) == Some(false) {
        for adapter in &mut inventory {
            adapter.ipv6 = false;
        }
    }
    Context::resolve(settings, &inventory)
}
pub async fn apply_policy(config: Mapping, requested: Option<&Settings>) -> Result<Mapping> {
    let current = settings().await;
    let settings = requested.unwrap_or(&current);
    if !settings.enabled {
        return Ok(config);
    }
    let ctx = context_for(
        settings,
        Some(config.get("ipv6").and_then(|v| v.as_bool()).unwrap_or(true)),
    )
    .await?;
    let (config, report) = clash_verge_dualnic::transform(config, &ctx)?;
    let _ = REPORT_CAPTURE.try_with(|capture| *capture.lock() = Some(report));
    Ok(config)
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub settings: Settings,
    pub wired: Option<Adapter>,
    pub proxy: Option<Adapter>,
    pub state: String,
    pub routes: Routes,
    pub error: Option<String>,
}
pub async fn status() -> Result<Status> {
    let s = settings().await;
    let ctx = if s.enabled || (!s.wired_guid.is_empty() && !s.proxy_guid.is_empty()) {
        context(&s).await?
    } else {
        Context::default()
    };
    let state = if !s.enabled {
        "disabled"
    } else if ctx.wired_ready() && ctx.proxy_ready() {
        "ready"
    } else if ctx.wired_ready() {
        "proxyOffline"
    } else if ctx.proxy_ready() {
        "wiredOffline"
    } else if ctx.unmatched_ready() {
        "unmatchedOnly"
    } else {
        "offline"
    };
    Ok(Status {
        routes: ctx.routes(),
        settings: s,
        wired: ctx.wired,
        proxy: ctx.proxy,
        state: state.into(),
        error: LAST_ERROR.lock().clone(),
    })
}
fn changed(error: Option<String>) {
    *LAST_ERROR.lock() = error;
    crate::process::AsyncHandler::spawn(|| async {
        let _ = crate::core::tray::Tray::global().update_tooltip().await;
    });
    if let Some(app) = crate::APP_HANDLE.get() {
        NotificationSystem::send_event(app.clone(), FrontendEvent::DualNicStatusChanged);
    }
}
#[derive(Serialize)]
pub struct Preview {
    pub yaml: String,
    pub validation: ValidationOutcome,
    pub report: Option<Report>,
}
pub async fn preview(s: Settings) -> Result<Preview> {
    let _write = Config::lock_config_write().await;
    let profiles = Config::profiles().await.latest_arc();
    let capture = std::sync::Arc::new(Mutex::new(None));
    let (mut config, _, _, _) = REPORT_CAPTURE
        .scope(
            std::sync::Arc::clone(&capture),
            crate::enhance::enhance_with_dual_nic(&profiles, Some(&s)),
        )
        .await?;
    crate::core::runtime_bundle::resolve_provider_path_conflicts(&mut config, &dirs::app_home_dir()?)?;
    let report = capture.lock().clone();
    let yaml = serde_yaml_ng::to_string(&config)?;
    let validation = CoreConfigValidator::global()
        .validate_config_outcome_with(&yaml)
        .await?;
    Ok(Preview {
        yaml,
        validation,
        report,
    })
}

pub(crate) async fn verify_running(s: &Settings) -> Result<()> {
    let version = Handle::mihomo().get_version().await?;
    ensure!(
        version.version.trim_start_matches('v') == "1.19.32",
        "The custom build requires Mihomo 1.19.32"
    );
    if !s.enabled {
        return Ok(());
    }
    let ctx = context(s).await?;
    let base = Handle::mihomo().get_base_config().await?;
    ensure!(
        base.mode.to_string().eq_ignore_ascii_case("rule")
            && base.tun.enable
            && base.tun.auto_route
            && base.tun.auto_detect_interface
            && base.tun.strict_route == Some(true),
        "Runtime rule mode or TUN settings differ from the dual NIC policy"
    );
    let live = Handle::mihomo().get_proxies().await?;
    if ctx.wired_ready() {
        let expected = &ctx.wired_effective().unwrap().name;
        ensure!(
            live.proxies.get(CN).is_some_and(|p| &p.interface == expected),
            "Wired binding verification failed"
        );
    }
    if ctx.unmatched_ready() {
        ensure!(
            live.proxies
                .get(UNMATCHED)
                .is_some_and(|p| p.interface == ctx.unmatched.as_ref().unwrap().name),
            "Unmatched direct binding verification failed"
        );
    }
    let rules_response = Handle::mihomo()
        .load_ctx()
        .build_request(reqwest::Method::GET, "/rules")?
        .send()
        .await?
        .error_for_status()?;
    let rules: serde_json::Value = rules_response.json().await?;
    let matches = rules
        .get("rules")
        .and_then(|r| r.as_array())
        .ok_or_else(|| anyhow::anyhow!("Runtime rules are unavailable"))?
        .iter()
        .filter(|r| {
            r.get("type")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t.eq_ignore_ascii_case("match"))
        })
        .filter_map(|r| r.get("proxy").and_then(|p| p.as_str()))
        .collect::<Vec<_>>();
    ensure!(
        matches
            == if ctx.unmatched_ready() {
                vec![UNMATCHED, "REJECT"]
            } else {
                vec!["REJECT"]
            },
        "Runtime default rules differ from the explicit fallback policy"
    );
    let runtime = Config::runtime().await.latest_arc();
    let lan_directs = runtime
        .config
        .as_ref()
        .and_then(|c| c.get("proxies"))
        .and_then(|p| p.as_sequence())
        .map(|p| {
            p.iter()
                .filter(|p| p.get("type").and_then(|v| v.as_str()) == Some("direct"))
                .filter_map(|p| p.get("name").and_then(|v| v.as_str()))
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    if !ctx.proxy_ready() {
        ensure!(
            live.proxies.iter().all(|(name, p)| p.all.is_some()
                || lan_directs.contains(name.as_str())
                || matches!(
                    name.as_str(),
                    "DIRECT" | "COMPATIBLE" | "REJECT" | "REJECT-DROP" | "PASS" | "PASS-RULE"
                )),
            "Proxy transports remain while the configured proxy output is blocked"
        );
    }
    if ctx.proxy_ready() {
        let expected = &ctx.proxy_effective().unwrap().name;
        ensure!(
            live.proxies.get(USB).is_some_and(|p| &p.interface == expected),
            "USB DNS binding verification failed"
        );
        for (name, p) in &live.proxies {
            if p.all.is_none()
                && !lan_directs.contains(name.as_str())
                && name != CN
                && name != UNMATCHED
                && name != "DIRECT"
                && name != "COMPATIBLE"
                && name != "REJECT"
                && name != "REJECT-DROP"
                && name != "PASS"
                && name != "PASS-RULE"
            {
                ensure!(
                    &p.interface == expected && !p.mptcp && p.dialer_proxy.is_empty(),
                    "USB proxy binding verification failed: {name}"
                );
            }
        }
    }
    Ok(())
}
pub async fn apply(mut requested: Settings) -> Result<Status> {
    let write = Config::lock_config_write().await;
    ensure_exclusive()?;
    requested.version = match requested.version {
        1 | 2 => 2,
        _ => anyhow::bail!("Unsupported dual NIC settings version"),
    };
    let previous = settings().await;
    let original_runtime = Config::runtime().await.data_arc();
    if requested.enabled {
        context(&requested).await?;
        requested.restore_tun = if previous.enabled {
            previous.restore_tun
        } else {
            Config::verge().await.latest_arc().enable_tun_mode
        };
    } else {
        requested.restore_tun = previous.restore_tun;
    }
    let patch = IVerge {
        dual_nic: Some(requested.clone()),
        enable_tun_mode: if requested.enabled {
            Some(true)
        } else {
            previous.restore_tun.or(Some(false))
        },
        auto_check_update: Some(false),
        ..Default::default()
    };
    let result = crate::feat::config::apply_verge_patch_locked(&write, &patch, false).await;
    let result = match result {
        Ok(()) => verify_running(&requested).await,
        Err(e) => Err(e),
    };
    if let Err(error) = result {
        if std::sync::Arc::ptr_eq(&original_runtime, &Config::runtime().await.data_arc()) {
            changed(Some(format!("{error:#}")));
            return Err(error);
        }
        let rollback = IVerge {
            dual_nic: Some(previous.clone()),
            enable_tun_mode: if previous.enabled {
                Some(true)
            } else {
                requested.restore_tun.or(Some(false))
            },
            ..Default::default()
        };
        let restored = crate::feat::config::apply_verge_patch_locked(&write, &rollback, false).await;
        changed(Some(format!("{error:#}")));
        if let Err(rollback_error) = restored {
            let stopped = CoreManager::global().stop_core().await;
            let detail =
                format!("Apply failed: {error:#}; rollback failed: {rollback_error:#}; stop result: {stopped:?}");
            changed(Some(detail.clone()));
            anyhow::bail!(detail);
        }
        return Err(error);
    }
    drop(write);
    changed(None);
    status().await
}

async fn close_affected(wired_changed: bool, proxy_changed: bool, unmatched_changed: bool) -> Result<()> {
    let connections = Handle::mihomo().get_connections().await?;
    let runtime = Config::runtime().await.data_arc();
    let lan_names = runtime
        .config
        .as_ref()
        .and_then(|c| c.get("proxies"))
        .and_then(|p| p.as_sequence())
        .map(|p| {
            p.iter()
                .filter(|p| p.get("type").and_then(|v| v.as_str()) == Some("direct"))
                .filter_map(|p| p.get("name").and_then(|v| v.as_str()))
                .filter(|n| *n != CN && *n != USB && *n != UNMATCHED)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    for c in connections.connections.unwrap_or_default() {
        let domestic = c.chains.iter().any(|n| n == CN);
        let local = c
            .chains
            .iter()
            .any(|n| lan_names.contains(n.as_str()) || matches!(n.as_str(), "DIRECT" | "COMPATIBLE"));
        let unmatched = c.chains.iter().any(|n| n == UNMATCHED);
        if should_close_route(
            domestic,
            unmatched,
            local,
            wired_changed,
            proxy_changed,
            unmatched_changed,
        ) {
            Handle::mihomo().close_connection(&c.id).await?;
        }
    }
    Ok(())
}

fn should_close_route(
    domestic: bool,
    unmatched: bool,
    local: bool,
    wired_changed: bool,
    proxy_changed: bool,
    unmatched_changed: bool,
) -> bool {
    if domestic {
        wired_changed
    } else if unmatched {
        unmatched_changed
    } else {
        !local && proxy_changed
    }
}

pub fn start() {
    #[cfg(windows)]
    {
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        crate::process::AsyncHandler::spawn(move || async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let notifications = clash_verge_dualnic::windows::watch(move || {
                let _ = tx.send(());
            });
            if let Err(e) = &notifications {
                changed(Some(format!("{e:#}")));
            }
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            let mut fingerprint = String::new();
            let mut conflict = false;
            let mut suspended = false;
            let mut previous_context: Option<Context> = None;
            loop {
                tokio::select! {_=interval.tick()=>{},event=rx.recv(),if notifications.is_ok()=>{if event.is_none(){break;}tokio::time::sleep(std::time::Duration::from_secs(1)).await;while rx.try_recv().is_ok(){}}}
                if GENERATION.load(Ordering::SeqCst) != generation || Handle::global().is_exiting() {
                    break;
                }
                if let Err(e) = ensure_exclusive() {
                    if !conflict {
                        match CoreManager::global().stop_core().await {
                            Ok(()) => {
                                changed(Some(format!("{e:#}")));
                                conflict = true;
                            }
                            Err(stop) => changed(Some(format!("{e:#}; stopping the custom core failed: {stop:#}"))),
                        }
                    }
                    continue;
                }
                if conflict || suspended {
                    match CoreManager::global().restart_core().await {
                        Ok(()) => {
                            conflict = false;
                            suspended = false;
                            fingerprint.clear();
                        }
                        Err(e) => {
                            changed(Some(format!("Restarting the custom core failed: {e:#}")));
                            continue;
                        }
                    }
                }
                let result: Result<()> = async {
                    let write = Config::lock_config_write().await;
                    let s = settings().await;
                    let ctx = if s.enabled { Some(context(&s).await?) } else { None };
                    let next = serde_json::to_string(&(s.clone(), &ctx))?;
                    if next == fingerprint {
                        return Ok(());
                    }
                    if s.enabled {
                        let ctx = ctx.as_ref().unwrap();
                        close_affected(
                            previous_context
                                .as_ref()
                                .is_none_or(|p| p.wired_effective() != ctx.wired_effective()),
                            previous_context
                                .as_ref()
                                .is_none_or(|p| p.proxy_effective() != ctx.proxy_effective()),
                            previous_context.as_ref().is_none_or(|p| p.unmatched != ctx.unmatched),
                        )
                        .await?;
                        let patch = IVerge {
                            dual_nic: Some(s.clone()),
                            ..Default::default()
                        };
                        crate::feat::config::apply_verge_patch_locked(&write, &patch, false).await?;
                        verify_running(&s).await?;
                    }
                    previous_context = ctx;
                    fingerprint = next;
                    changed(None);
                    Ok(())
                }
                .await;
                if let Err(e) = result {
                    if enabled().await {
                        match CoreManager::global().stop_core().await {
                            Ok(()) => {
                                suspended = true;
                                changed(Some(format!(
                                    "{e:#}; the custom core was stopped to preserve NIC isolation"
                                )));
                            }
                            Err(stop) => changed(Some(format!("{e:#}; stopping the custom core failed: {stop:#}"))),
                        }
                    } else {
                        changed(Some(format!("{e:#}")));
                    }
                }
            }
            drop(notifications);
        });
    }
}
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

#[derive(Serialize)]
pub struct EgressEvidence {
    pub label: String,
    pub interface: String,
    pub source_addresses: Vec<String>,
    pub status: String,
    pub detail: String,
}
#[derive(Serialize)]
pub struct Detection {
    pub configured: bool,
    pub configuration_error: Option<String>,
    pub results: Vec<(String, bool, String)>,
    pub egress: Vec<EgressEvidence>,
}

async fn dns_addresses(domain: &str) -> Result<Vec<std::net::IpAddr>> {
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        let mut url = reqwest::Url::parse("http://localhost/dns/query")?;
        url.query_pairs_mut()
            .append_pair("name", domain)
            .append_pair("type", "A");
        let path = format!("{}?{}", url.path(), url.query().unwrap_or(""));
        let response = Handle::mihomo()
            .load_ctx()
            .build_request(reqwest::Method::GET, &path)?
            .send()
            .await?
            .error_for_status()?;
        let answer: serde_json::Value = response.json().await?;
        let addresses = answer
            .get("Answer")
            .and_then(|a| a.as_array())
            .into_iter()
            .flatten()
            .filter_map(|a| a.get("data").and_then(|a| a.as_str()))
            .filter_map(|ip| ip.parse().ok())
            .collect::<Vec<_>>();
        ensure!(!addresses.is_empty(), "DNS returned no address");
        Ok(addresses)
    })
    .await
    .map_err(|_| anyhow::anyhow!("DNS query timed out"))?
}

#[cfg(windows)]
fn matching_sources(
    sockets: &[clash_verge_dualnic::windows::TcpSocket],
    targets: &[std::net::IpAddr],
    port: u16,
    addresses: &[String],
) -> Vec<String> {
    sockets
        .iter()
        .filter(|socket| {
            targets.contains(&socket.remote_ip)
                && socket.remote_port == port
                && addresses
                    .iter()
                    .any(|ip| ip.parse::<std::net::IpAddr>().ok() == Some(socket.local_ip))
        })
        .map(|socket| socket.local_ip.to_string())
        .collect()
}

async fn probe_with_evidence(
    name: &str,
    url: &str,
    label: &str,
    adapter: Option<&Adapter>,
    pid: Option<u32>,
    targets: Vec<std::net::IpAddr>,
    port: u16,
) -> ((String, bool, String), EgressEvidence) {
    let physical = tokio::task::spawn_blocking(adapters)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut evidence = EgressEvidence {
        label: label.into(),
        interface: adapter.map_or_else(String::new, |a| a.name.clone()),
        source_addresses: Vec::new(),
        status: "notObserved".into(),
        detail: "未采集到目标 TCP 连接的物理源地址；UDP 节点需要抓包确认。".into(),
    };
    let probe = async {
        let result = Handle::mihomo().delay_proxy_by_name(name, url, 5000).await;
        done.store(true, Ordering::Release);
        (
            label.into(),
            result.is_ok(),
            result
                .map(|r| format!("{} ms", r.delay))
                .unwrap_or_else(|e| e.to_string()),
        )
    };
    let observe = async {
        #[cfg(windows)]
        if let (Some(pid), Some(_adapter)) = (pid, adapter) {
            if !targets.is_empty() {
                let started = std::time::Instant::now();
                loop {
                    match clash_verge_dualnic::windows::tcp_sockets(pid) {
                        Ok(sockets) => evidence.source_addresses.extend(
                            sockets
                                .iter()
                                .filter(|socket| targets.contains(&socket.remote_ip) && socket.remote_port == port)
                                .map(|socket| socket.local_ip.to_string()),
                        ),
                        Err(e) => {
                            evidence.status = "unavailable".into();
                            evidence.detail = e.to_string();
                            break;
                        }
                    }
                    if done.load(Ordering::Acquire) || started.elapsed() > std::time::Duration::from_secs(6) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }
        } else {
            evidence.status = "unavailable".into();
            evidence.detail = "未获取当前会话的内核 PID，无法核对物理源地址。".into();
        }
        evidence.source_addresses.sort();
        evidence.source_addresses.dedup();
        if !evidence.source_addresses.is_empty() {
            let unambiguous = evidence
                .source_addresses
                .iter()
                .all(|ip| physical.iter().filter(|a| a.addresses.contains(ip)).count() == 1);
            let matches = adapter.is_some_and(|a| evidence.source_addresses.iter().all(|ip| a.addresses.contains(ip)));
            evidence.status = if !unambiguous {
                "unavailable"
            } else if matches {
                "verified"
            } else {
                "mismatch"
            }
            .into();
            evidence.detail = if !unambiguous {
                "源地址无法唯一关联物理网卡，不能作为出口证明。"
            } else if matches {
                "已按自有内核 PID、目标地址和端口核对真实 TCP socket。"
            } else {
                "目标 TCP 连接源地址与配置出口不符。"
            }
            .into();
        }
        evidence
    };
    tokio::join!(probe, observe)
}

pub async fn test() -> Result<Detection> {
    let s = settings().await;
    ensure!(s.enabled, "Enable dual NIC before testing");
    let verification = verify_running(&s).await;
    let configured = verification.is_ok();
    let configuration_error = verification.err().map(|e| format!("{e:#}"));
    let ctx = context(&s).await?;
    let pid = crate::core::service::owned_core_pid().await;
    let mut results = Vec::new();
    let mut egress = Vec::new();
    let domestic = dns_addresses("www.baidu.com").await.unwrap_or_default();
    let (result, evidence) = probe_with_evidence(
        CN,
        "https://www.baidu.com",
        "国内连接",
        ctx.wired_effective(),
        pid,
        domestic,
        443,
    )
    .await;
    results.push(result);
    egress.push(evidence);
    let runtime = Config::runtime().await.data_arc();
    let default = runtime
        .config
        .as_ref()
        .and_then(|c| c.get("dns"))
        .and_then(|d| d.get("nameserver"))
        .and_then(|v| v.as_sequence())
        .and_then(|v| v.first())
        .and_then(|v| v.as_str())
        .and_then(|s| s.rsplit_once('#'))
        .map(|(_, n)| n.to_owned());
    if let Some(name) = default.filter(|_| ctx.proxy_ready()) {
        let live = Handle::mihomo().get_proxies().await?;
        let mut leaf = name.clone();
        let mut seen = std::collections::HashSet::new();
        while seen.insert(leaf.clone()) {
            let Some(next) = live.proxies.get(leaf.as_str()).and_then(|p| p.now.as_deref()) else {
                break;
            };
            leaf = next.to_owned();
        }
        let node = runtime
            .config
            .as_ref()
            .and_then(|c| c.get("proxies"))
            .and_then(|p| p.as_sequence())
            .and_then(|p| {
                p.iter()
                    .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(leaf.as_str()))
            });
        let server = node
            .and_then(|p| p.get("server"))
            .and_then(|s| s.as_str())
            .unwrap_or("");
        let port = node.and_then(|p| p.get("port")).and_then(|p| p.as_u64()).unwrap_or(0) as u16;
        let targets = if let Ok(ip) = server.parse() {
            vec![ip]
        } else if !server.is_empty() {
            dns_addresses(server).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let (result, evidence) = probe_with_evidence(
            &name,
            "https://www.gstatic.com/generate_204",
            "代理连接",
            ctx.proxy_effective(),
            pid,
            targets,
            port,
        )
        .await;
        results.push(result);
        egress.push(evidence);
    }
    if let Some(adapter) = ctx.unmatched.as_ref().filter(|a| a.ready()) {
        let targets = dns_addresses("www.cloudflare.com").await.unwrap_or_default();
        let (result, evidence) = probe_with_evidence(
            UNMATCHED,
            "https://www.cloudflare.com/",
            "默认直连出口探测",
            Some(adapter),
            pid,
            targets,
            443,
        )
        .await;
        results.push(result);
        egress.push(evidence);
    }
    for (label, domain) in [("国内 DNS", "www.baidu.com"), ("国外 DNS", "www.cloudflare.com")] {
        let outcome = dns_addresses(domain).await;
        results.push((
            label.into(),
            outcome.is_ok(),
            outcome
                .map(|a| a.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|e| e.to_string()),
        ));
    }
    Ok(Detection {
        configured,
        configuration_error,
        results,
        egress,
    })
}

#[cfg(all(test, windows))]
mod diagnostic_tests {
    #[test]
    fn physical_evidence_requires_the_expected_destination_port_and_source() {
        use clash_verge_dualnic::windows::TcpSocket;
        let target = "203.0.113.7".parse().unwrap();
        let addresses = vec!["192.168.115.21".into()];
        let samples = vec![
            TcpSocket {
                local_ip: "198.18.0.1".parse().unwrap(),
                local_port: 12345,
                remote_ip: target,
                remote_port: 443,
            },
            TcpSocket {
                local_ip: "192.168.115.21".parse().unwrap(),
                local_port: 12345,
                remote_ip: target,
                remote_port: 53,
            },
            TcpSocket {
                local_ip: "192.168.115.21".parse().unwrap(),
                local_port: 12345,
                remote_ip: "203.0.113.8".parse().unwrap(),
                remote_port: 443,
            },
            TcpSocket {
                local_ip: "192.168.115.21".parse().unwrap(),
                local_port: 12345,
                remote_ip: target,
                remote_port: 443,
            },
        ];
        assert_eq!(
            super::matching_sources(&samples, &[target], 443, &addresses),
            vec!["192.168.115.21"]
        );
    }
}

pub async fn proxy_offline() -> bool {
    let s = settings().await;
    s.enabled && context(&s).await.is_ok_and(|ctx| !ctx.proxy_ready())
}

#[derive(Serialize)]
pub struct SocketObservation {
    pub local_ip: String,
    pub remote_ip: String,
    pub remote_port: u16,
    pub local_port: u16,
    pub adapter_guids: Vec<String>,
    pub role: Option<String>,
    pub expected_guid: Option<String>,
    pub status: String,
}
#[derive(Serialize)]
pub struct TrafficSnapshot {
    pub sampled_at_ms: u64,
    pub adapters: Vec<clash_verge_dualnic::AdapterCounters>,
    pub status: Status,
    pub core_pid: Option<u32>,
    pub sockets: Vec<SocketObservation>,
    pub socket_error: Option<String>,
}
#[cfg(windows)]
fn observe_socket(
    socket: &clash_verge_dualnic::windows::TcpSocket,
    adapters: &[clash_verge_dualnic::AdapterCounters],
    candidates: &[(std::net::IpAddr, u16, String)],
    routes: &Routes,
) -> SocketObservation {
    let adapter_guids = adapters
        .iter()
        .filter(|a| {
            a.adapter
                .addresses
                .iter()
                .any(|ip| ip.parse::<std::net::IpAddr>().ok() == Some(socket.local_ip))
        })
        .map(|a| a.adapter.guid.clone())
        .collect::<Vec<_>>();
    let roles = candidates
        .iter()
        .filter(|(ip, port, _)| *ip == socket.remote_ip && *port == socket.remote_port)
        .map(|(_, _, role)| role.as_str())
        .collect::<std::collections::HashSet<_>>();
    let role = if roles.len() == 1 {
        roles.into_iter().next().map(str::to_owned)
    } else {
        None
    };
    let route = match role.as_deref() {
        Some("domestic") => Some(&routes.domestic),
        Some("proxy") => Some(&routes.proxy),
        Some("unmatched") => Some(&routes.unmatched),
        _ => None,
    };
    let expected_guid = route.and_then(|r| r.effective.as_ref()).map(|a| a.guid.clone());
    let status = if route.is_none() || adapter_guids.len() != 1 {
        "observed"
    } else if expected_guid.as_ref().is_some_and(|guid| adapter_guids.contains(guid)) {
        "verified"
    } else {
        "mismatch"
    };
    SocketObservation {
        local_ip: socket.local_ip.to_string(),
        remote_ip: socket.remote_ip.to_string(),
        remote_port: socket.remote_port,
        local_port: socket.local_port,
        adapter_guids,
        role,
        expected_guid,
        status: status.into(),
    }
}
pub async fn traffic_snapshot() -> Result<TrafficSnapshot> {
    #[cfg(windows)]
    {
        static STARTED: LazyLock<std::time::Instant> = LazyLock::new(std::time::Instant::now);
        let _ = *STARTED;
        let adapters = tokio::task::spawn_blocking(clash_verge_dualnic::windows::network_counters).await??;
        let sampled_at_ms = STARTED.elapsed().as_millis() as u64;
        let current = status().await?;
        let pid = crate::core::service::owned_core_pid().await;
        let mut candidates = Vec::new();
        if current.settings.enabled && pid.is_some() {
            let runtime = Config::runtime().await.data_arc();
            if let Some(proxies) = runtime
                .config
                .as_ref()
                .and_then(|c| c.get("proxies"))
                .and_then(|p| p.as_sequence())
            {
                for node in proxies
                    .iter()
                    .filter(|p| p.get("type").and_then(|v| v.as_str()) != Some("direct"))
                {
                    if let (Some(ip), Some(port)) = (
                        node.get("server").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()),
                        node.get("port")
                            .and_then(|v| v.as_u64())
                            .and_then(|p| u16::try_from(p).ok()),
                    ) {
                        candidates.push((ip, port, "proxy".into()));
                    }
                }
            }
            if let Ok(Ok(connections)) = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                Handle::mihomo().get_connections(),
            )
            .await
            {
                for connection in connections.connections.unwrap_or_default() {
                    let role = if connection.chains.iter().any(|n| n == CN) {
                        "domestic"
                    } else if connection.chains.iter().any(|n| n == UNMATCHED) {
                        "unmatched"
                    } else {
                        continue;
                    };
                    if let (Ok(ip), Ok(port)) = (
                        connection.metadata.destination_ip.parse(),
                        connection.metadata.destination_port.parse(),
                    ) {
                        candidates.push((ip, port, role.into()));
                    }
                }
            }
        }
        let (sockets, socket_error) = if let Some(pid) = pid {
            match tokio::task::spawn_blocking(move || clash_verge_dualnic::windows::tcp_sockets(pid)).await? {
                Ok(sockets) => (
                    sockets
                        .iter()
                        .map(|s| observe_socket(s, &adapters, &candidates, &current.routes))
                        .collect(),
                    None,
                ),
                Err(e) => (Vec::new(), Some(e.to_string())),
            }
        } else {
            (Vec::new(), Some("未获取当前自有内核 PID；网卡流量仍可观察。".into()))
        };
        Ok(TrafficSnapshot {
            sampled_at_ms,
            adapters,
            status: current,
            core_pid: pid,
            sockets,
            socket_error,
        })
    }
    #[cfg(not(windows))]
    anyhow::bail!("Adapter traffic is available on Windows only")
}

#[cfg(all(test, windows))]
mod route_tests {
    use super::*;
    #[test]
    fn route_changes_close_managed_defaults_but_keep_lan_connections() {
        assert!(should_close_route(true, false, false, true, false, false));
        assert!(should_close_route(false, true, false, false, false, true));
        assert!(should_close_route(false, false, false, false, true, false));
        assert!(!should_close_route(false, false, true, true, true, true));
        assert!(!should_close_route(false, true, false, true, true, false));
    }
    #[test]
    fn migration_keeps_old_primary_choices_and_leaves_new_outputs_empty() {
        let config: IVerge = serde_yaml_ng::from_str(
            "dual_nic:\n  version: 1\n  enabled: true\n  wired_guid: wired\n  proxy_guid: proxy\n",
        )
        .unwrap();
        let settings = config.dual_nic.unwrap();
        assert_eq!(settings.version, 2);
        assert_eq!(settings.wired_guid, "wired");
        assert_eq!(settings.proxy_guid, "proxy");
        assert!(
            settings.wired_backup_guid.is_none()
                && settings.proxy_backup_guid.is_none()
                && settings.unmatched_guid.is_none()
        );
    }
    #[test]
    fn tcp_evidence_matches_a_destination_and_can_report_the_wrong_adapter() {
        use clash_verge_dualnic::windows::TcpSocket;
        use clash_verge_dualnic::{Adapter, AdapterCounters};
        let adapter = |name: &str, ip: &str| Adapter {
            guid: name.into(),
            name: name.into(),
            description: name.into(),
            up: true,
            addresses: vec![ip.into()],
            gateways: vec!["192.0.2.1".into()],
            ipv4: true,
            ipv6: false,
        };
        let expected = adapter("expected", "192.0.2.2");
        let other = adapter("other", "192.0.2.3");
        let rows = vec![expected.clone(), other]
            .into_iter()
            .map(|adapter| AdapterCounters {
                adapter,
                physical: true,
                interface_id: "1".into(),
                received: "0".into(),
                sent: "0".into(),
            })
            .collect::<Vec<_>>();
        let routes = Context {
            proxy: Some(expected),
            ..Default::default()
        }
        .routes();
        let socket = TcpSocket {
            local_ip: "192.0.2.3".parse().unwrap(),
            local_port: 12345,
            remote_ip: "203.0.113.1".parse().unwrap(),
            remote_port: 443,
        };
        let candidates = vec![(socket.remote_ip, 443, "proxy".into())];
        assert_eq!(observe_socket(&socket, &rows, &candidates, &routes).status, "mismatch");
        let right = TcpSocket {
            local_ip: "192.0.2.2".parse().unwrap(),
            ..socket.clone()
        };
        assert_eq!(observe_socket(&right, &rows, &candidates, &routes).status, "verified");
        assert_eq!(observe_socket(&socket, &rows, &[], &routes).status, "observed");
        let ambiguous = vec![
            (socket.remote_ip, 443, "proxy".into()),
            (socket.remote_ip, 443, "domestic".into()),
        ];
        assert_eq!(observe_socket(&socket, &rows, &ambiguous, &routes).status, "observed");
    }
}
