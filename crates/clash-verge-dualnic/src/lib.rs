use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};
use std::collections::{HashMap, HashSet};

#[cfg(windows)]
pub mod windows;

pub const CN: &str = "__DualNic_CN";
pub const USB: &str = "__DualNic_USB_DNS";
pub const FALLBACK: &str = "__DualNic_Fallback";
pub const UNMATCHED: &str = "__DualNic_Unmatched";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub version: u8,
    pub enabled: bool,
    pub wired_guid: String,
    pub proxy_guid: String,
    pub restore_tun: Option<bool>,
    pub wired_backup_guid: Option<String>,
    pub proxy_backup_guid: Option<String>,
    pub unmatched_guid: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 2,
            enabled: false,
            wired_guid: String::new(),
            proxy_guid: String::new(),
            restore_tun: None,
            wired_backup_guid: None,
            proxy_backup_guid: None,
            unmatched_guid: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Adapter {
    pub guid: String,
    pub name: String,
    pub description: String,
    pub up: bool,
    pub addresses: Vec<String>,
    pub gateways: Vec<String>,
    pub ipv4: bool,
    pub ipv6: bool,
}
impl Adapter {
    pub fn ready(&self) -> bool {
        self.up && (self.ipv4 || self.ipv6)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Context {
    pub wired: Option<Adapter>,
    pub proxy: Option<Adapter>,
    pub wired_backup: Option<Adapter>,
    pub proxy_backup: Option<Adapter>,
    pub unmatched: Option<Adapter>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Route {
    pub primary: Option<Adapter>,
    pub backup: Option<Adapter>,
    pub effective: Option<Adapter>,
    pub state: String,
    pub reason: String,
}
impl Route {
    pub fn resolve(primary: &Option<Adapter>, backup: &Option<Adapter>) -> Self {
        let (effective, state, reason) = if primary.as_ref().is_some_and(Adapter::ready) {
            (primary.clone(), "primary", "primaryAvailable")
        } else if backup.as_ref().is_some_and(Adapter::ready) {
            (backup.clone(), "backup", "primaryUnavailable")
        } else {
            (None, "blocked", "noAvailableAdapter")
        };
        Self {
            primary: primary.clone(),
            backup: backup.clone(),
            effective,
            state: state.into(),
            reason: reason.into(),
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Routes {
    pub domestic: Route,
    pub proxy: Route,
    pub unmatched: Route,
}
impl Context {
    pub fn resolve(settings: &Settings, adapters: &[Adapter]) -> Result<Self> {
        if !matches!(settings.version, 1 | 2) {
            bail!("Unsupported dual NIC settings version");
        }
        if settings.wired_guid.is_empty() || settings.proxy_guid.is_empty() {
            bail!("Select the domestic and proxy physical adapters");
        }
        let find = |guid: &str| adapters.iter().find(|a| a.guid.eq_ignore_ascii_case(guid)).cloned();
        Ok(Self {
            wired: find(&settings.wired_guid),
            proxy: find(&settings.proxy_guid),
            wired_backup: settings.wired_backup_guid.as_deref().and_then(find),
            proxy_backup: settings.proxy_backup_guid.as_deref().and_then(find),
            unmatched: settings.unmatched_guid.as_deref().and_then(find),
        })
    }
    pub fn wired_effective(&self) -> Option<&Adapter> {
        self.wired
            .as_ref()
            .filter(|a| a.ready())
            .or_else(|| self.wired_backup.as_ref().filter(|a| a.ready()))
    }
    pub fn proxy_effective(&self) -> Option<&Adapter> {
        self.proxy
            .as_ref()
            .filter(|a| a.ready())
            .or_else(|| self.proxy_backup.as_ref().filter(|a| a.ready()))
    }
    pub fn wired_ready(&self) -> bool {
        self.wired_effective().is_some()
    }
    pub fn proxy_ready(&self) -> bool {
        self.proxy_effective().is_some()
    }
    pub fn unmatched_ready(&self) -> bool {
        self.unmatched.as_ref().is_some_and(Adapter::ready)
    }
    pub fn routes(&self) -> Routes {
        Routes {
            domestic: Route::resolve(&self.wired, &self.wired_backup),
            proxy: Route::resolve(&self.proxy, &self.proxy_backup),
            unmatched: Route::resolve(&self.unmatched, &None),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    pub proxy_dns_group: String,
    pub routes: Routes,
    pub proxy_count: usize,
    pub provider_count: usize,
    pub rewritten_direct_rules: Vec<String>,
    pub skipped_nodes: Vec<String>,
    pub wired_ready: bool,
    pub proxy_ready: bool,
}

fn text<'a>(m: &'a Mapping, k: &str) -> &'a str {
    m.get(k).and_then(Value::as_str).unwrap_or("")
}
fn seq(m: &Mapping, k: &str) -> Vec<Value> {
    m.get(k).and_then(Value::as_sequence).cloned().unwrap_or_default()
}
fn object(m: &Mapping, k: &str) -> Mapping {
    m.get(k).and_then(Value::as_mapping).cloned().unwrap_or_default()
}
fn put(m: &mut Mapping, k: &str, v: impl Into<Value>) {
    m.insert(k.into(), v.into());
}
fn strings(v: &[Value]) -> Vec<String> {
    v.iter().filter_map(Value::as_str).map(str::to_owned).collect()
}
fn string_values(v: Vec<String>) -> Value {
    Value::Sequence(v.into_iter().map(Value::from).collect())
}
fn direct_names(proxies: &[Value]) -> HashSet<String> {
    let mut names = HashSet::from(["DIRECT".to_owned(), "COMPATIBLE".to_owned()]);
    for p in proxies {
        if let Some(m) = p.as_mapping() {
            if text(m, "type") == "direct" {
                names.insert(text(m, "name").to_owned());
            }
        }
    }
    names
}
fn tokens(rule: &str) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0;
    for (i, c) in rule.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    bail!("Unbalanced rule: {rule}");
                }
            }
            ',' if depth == 0 => {
                parts.push(rule[start..i].trim().to_owned());
                start = i + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        bail!("Unbalanced rule: {rule}");
    }
    parts.push(rule[start..].trim().to_owned());
    if parts.len() < 2 {
        bail!("Invalid rule: {rule}");
    }
    Ok(parts)
}
fn target_index(parts: &[String]) -> usize {
    if parts[0].eq_ignore_ascii_case("MATCH") || parts[0].eq_ignore_ascii_case("FINAL") {
        1
    } else {
        2.min(parts.len() - 1)
    }
}
fn local(rule: &str) -> bool {
    let Ok(p) = tokens(rule) else {
        return false;
    };
    if p.iter()
        .skip(target_index(&p) + 1)
        .any(|v| v.eq_ignore_ascii_case("src"))
    {
        return false;
    }
    let kind = p[0].to_uppercase();
    let value = p.get(1).map(String::as_str).unwrap_or("").to_lowercase();
    match kind.as_str() {
        "GEOIP" | "GEOSITE" => value == "private" || value == "lan",
        "DOMAIN" | "DOMAIN-SUFFIX" => {
            value == "localhost"
                || value == "lan"
                || value == "local"
                || value == "home.arpa"
                || value.ends_with(".lan")
                || value.ends_with(".local")
                || value.ends_with(".home.arpa")
        }
        "IP-CIDR" | "IP-CIDR6" => private_cidr(&value),
        "AND" | "OR" => {
            let Some(terms) = value.strip_prefix('(').and_then(|s| s.strip_suffix(')')) else {
                return false;
            };
            let Ok(terms) = tokens(terms) else {
                return false;
            };
            let checks = terms.iter().map(|term| {
                term.strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                    .is_some_and(|s| local(&format!("{s},DIRECT")))
            });
            if kind == "AND" {
                checks.clone().any(|v| v)
            } else {
                checks.clone().all(|v| v)
            }
        }
        _ => false,
    }
}
fn private_cidr(value: &str) -> bool {
    let Some((address, bits)) = value.split_once('/') else {
        return false;
    };
    let Ok(bits) = bits.parse::<u8>() else {
        return false;
    };
    match address.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            let o = ip.octets();
            bits <= 32
                && ((o[0] == 10 && bits >= 8)
                    || (o[0] == 172 && (16..32).contains(&o[1]) && bits >= 12)
                    || (o[0] == 192 && o[1] == 168 && bits >= 16)
                    || (o[0] == 127 && bits >= 8)
                    || (o[0] == 169 && o[1] == 254 && bits >= 16))
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            bits <= 128
                && ((ip.is_loopback() && bits == 128)
                    || ((ip.segments()[0] & 0xfe00) == 0xfc00 && bits >= 7)
                    || ((ip.segments()[0] & 0xffc0) == 0xfe80 && bits >= 10))
        }
        _ => false,
    }
}
fn loopback(server: &str) -> bool {
    server.eq_ignore_ascii_case("localhost") || server.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}
fn reachable(
    name: &str,
    groups: &HashMap<String, Mapping>,
    leaves: &HashSet<String>,
    providers: &Mapping,
    visiting: &mut HashSet<String>,
) -> Result<bool> {
    if leaves.contains(name) {
        return Ok(true);
    }
    if matches!(
        name,
        "DIRECT" | "COMPATIBLE" | "REJECT" | "REJECT-DROP" | "PASS" | "PASS-RULE"
    ) {
        return Ok(false);
    }
    let Some(group) = groups.get(name) else {
        bail!("Unknown proxy reference: {name}");
    };
    if !visiting.insert(name.to_owned()) {
        bail!("Cyclic proxy group: {name}");
    }
    let mut found = false;
    for provider in strings(&seq(group, "use")) {
        if !providers.contains_key(provider.as_str()) {
            bail!("Unknown provider: {provider}");
        }
        found = true;
    }
    if group
        .get("include-all")
        .or(group.get("include-all-proxies"))
        .and_then(Value::as_bool)
        == Some(true)
        && !leaves.is_empty()
    {
        found = true;
    }
    if group.get("include-all-providers").and_then(Value::as_bool) == Some(true) && !providers.is_empty() {
        found = true;
    }
    for child in strings(&seq(group, "proxies")) {
        found |= reachable(&child, groups, leaves, providers, visiting)?;
    }
    visiting.remove(name);
    Ok(found)
}
fn bind(m: &mut Mapping, a: &Adapter) {
    put(m, "interface-name", a.name.clone());
    put(m, "mptcp", false);
    put(
        m,
        "ip-version",
        if a.ipv4 && a.ipv6 {
            "dual"
        } else if a.ipv6 {
            "ipv6"
        } else {
            "ipv4"
        },
    );
    m.remove("routing-mark");
}
fn helper(name: &str, a: &Adapter) -> Value {
    let mut m = Mapping::new();
    put(&mut m, "name", name);
    put(&mut m, "type", "direct");
    put(&mut m, "udp", true);
    bind(&mut m, a);
    Value::Mapping(m)
}

fn validate_subrules(subrules: &Mapping) -> Result<()> {
    fn visit(name: &str, all: &Mapping, active: &mut HashSet<String>, done: &mut HashSet<String>) -> Result<()> {
        if done.contains(name) {
            return Ok(());
        }
        if !active.insert(name.into()) {
            bail!("Cyclic sub-rule: {name}");
        }
        let rules = all
            .get(name)
            .and_then(Value::as_sequence)
            .ok_or_else(|| anyhow::anyhow!("Unknown or invalid sub-rule: {name}"))?;
        for value in rules {
            let r = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Non-string sub-rule: {name}"))?;
            let p = tokens(r)?;
            if p[0].eq_ignore_ascii_case("SUB-RULE") {
                visit(&p[target_index(&p)], all, active, done)?;
            }
        }
        active.remove(name);
        done.insert(name.into());
        Ok(())
    }
    let mut done = HashSet::new();
    for name in subrules.keys() {
        let name = name.as_str().ok_or_else(|| anyhow::anyhow!("Invalid sub-rule name"))?;
        visit(name, subrules, &mut HashSet::new(), &mut done)?;
    }
    Ok(())
}
fn priority_name(name: &str) -> String {
    format!("__DualNic_Priority_{name}")
}
fn priority_rule(rule: &str) -> Result<String> {
    let mut p = tokens(rule)?;
    let ti = target_index(&p);
    if p[0].eq_ignore_ascii_case("SUB-RULE") {
        p[ti] = priority_name(&p[ti]);
    } else if !local(rule) && !matches!(p[ti].as_str(), "REJECT" | "REJECT-DROP") {
        p[ti] = "PASS-RULE".into();
    }
    Ok(p.join(","))
}

fn guard_proxy_rules(rules: Vec<String>, proxy_names: &HashSet<String>) -> Result<(Vec<String>, Vec<String>)> {
    let original: HashSet<_> = rules.iter().cloned().collect();
    let mut guarded = Vec::new();
    let mut generated = Vec::new();
    for rule in rules {
        let mut parts = tokens(&rule)?;
        let target = target_index(&parts);
        let proxy = !parts[0].eq_ignore_ascii_case("SUB-RULE") && proxy_names.contains(&parts[target]);
        guarded.push(rule);
        if proxy {
            parts[target] = "REJECT".into();
            let guard = parts.join(",");
            if !original.contains(&guard) {
                guarded.push(guard.clone());
                generated.push(guard);
            }
        }
    }
    Ok((guarded, generated))
}

pub fn transform(mut config: Mapping, ctx: &Context) -> Result<(Mapping, Report)> {
    let ipv6_enabled = config.get("ipv6").and_then(Value::as_bool) != Some(false);
    let mut effective = ctx.clone();
    if !ipv6_enabled {
        for adapter in [
            &mut effective.wired,
            &mut effective.proxy,
            &mut effective.wired_backup,
            &mut effective.proxy_backup,
            &mut effective.unmatched,
        ]
        .into_iter()
        .flatten()
        {
            adapter.ipv6 = false;
        }
    }
    let routes = effective.routes();
    effective.wired = effective.wired_effective().cloned();
    effective.proxy = effective.proxy_effective().cloned();
    let ctx = &effective;
    if !seq(&config, "tunnels").is_empty() {
        bail!("Custom traffic tunnels are not supported together with dual NIC policy");
    }
    if seq(&config, "listeners")
        .iter()
        .any(|l| l.get("type").and_then(Value::as_str) == Some("tun"))
    {
        bail!("Additional TUN listeners conflict with the managed TUN");
    }
    let managed = config
        .get("x-dual-nic-version")
        .and_then(Value::as_u64)
        .is_some_and(|v| matches!(v, 1 | 2));
    let old_guards = if managed {
        object(&config, "x-dual-nic-udp-guards")
    } else {
        Mapping::new()
    };
    config.remove("x-dual-nic-udp-guards");
    let mut report = Report {
        routes,
        wired_ready: ctx.wired_ready(),
        proxy_ready: ctx.proxy_ready(),
        ..Default::default()
    };
    let original = seq(&config, "proxies");
    let directs = direct_names(&original);
    let mut lan_directs = Vec::new();
    let lan_targets: HashSet<_> = strings(&seq(&config, "rules"))
        .into_iter()
        .chain(
            object(&config, "sub-rules")
                .values()
                .filter_map(Value::as_sequence)
                .flat_map(|v| strings(v)),
        )
        .filter(|r| local(r))
        .filter_map(|r| tokens(&r).ok().map(|p| p[target_index(&p)].clone()))
        .collect();
    let mut proxies = Vec::new();
    let mut leaves = HashSet::new();
    let mut skipped = HashSet::new();
    for p in original {
        let Some(mut m) = p.as_mapping().cloned() else {
            bail!("Invalid proxy object");
        };
        let name = text(&m, "name").to_owned();
        if name.is_empty() {
            bail!("Unnamed proxy");
        }
        if [CN, USB, FALLBACK, UNMATCHED].contains(&name.as_str()) {
            if !managed {
                bail!("Reserved proxy name: {name}");
            }
            continue;
        }
        if directs.contains(&name) {
            if lan_targets.contains(&name) {
                lan_directs.push(Value::Mapping(m));
            }
            continue;
        }
        if loopback(text(&m, "server")) {
            skipped.insert(name.clone());
            report.skipped_nodes.push(name);
            continue;
        }
        if m.contains_key("dialer-proxy") {
            bail!("Proxy chains are incompatible with dual NIC: {name}");
        }
        if !leaves.insert(name.clone()) {
            bail!("Duplicate proxy name: {name}");
        }
        if let Some(a) = ctx.proxy.as_ref() {
            bind(&mut m, a);
        }
        proxies.push(Value::Mapping(m));
    }
    let mut providers = object(&config, "proxy-providers");
    for (name, p) in providers.iter_mut() {
        let Some(m) = p.as_mapping_mut() else {
            bail!("Invalid provider: {name:?}");
        };
        if m.contains_key("override-expr")
            || object(m, "override").contains_key("override-expr")
            || object(m, "override").contains_key("dialer-proxy")
        {
            bail!("Provider expression or proxy chain is incompatible: {name:?}");
        }
        let mut over = object(m, "override");
        if let Some(a) = ctx.proxy.as_ref() {
            bind(&mut over, a);
        }
        put(m, "override", over);
        let excluded = text(m, "exclude-type");
        put(
            m,
            "exclude-type",
            if excluded.is_empty() {
                "direct".to_owned()
            } else if excluded.split('|').any(|s| s.eq_ignore_ascii_case("direct")) {
                excluded.to_owned()
            } else {
                format!("{excluded}|direct")
            },
        );
        if text(m, "type") == "http" {
            put(m, "proxy", USB);
        }
    }
    let mut groups = Vec::new();
    let mut group_names = HashSet::new();
    for g in seq(&config, "proxy-groups") {
        let Some(mut m) = g.as_mapping().cloned() else {
            bail!("Invalid proxy group");
        };
        let name = text(&m, "name").to_owned();
        if [CN, USB, FALLBACK, UNMATCHED].contains(&name.as_str()) {
            if managed && name == FALLBACK {
                continue;
            }
            bail!("Reserved group name: {name}");
        }
        if name.is_empty() || !group_names.insert(name.clone()) || leaves.contains(&name) {
            bail!("Invalid or duplicate group: {name}");
        }
        if text(&m, "type") == "relay" {
            bail!("Proxy chains are incompatible: {name}");
        }
        let children = strings(&seq(&m, "proxies"))
            .into_iter()
            .filter(|n| {
                !directs.contains(n)
                    && !skipped.contains(n)
                    && n != CN
                    && n != USB
                    && n != UNMATCHED
                    && !matches!(n.as_str(), "PASS" | "PASS-RULE")
            })
            .collect();
        put(&mut m, "proxies", string_values(children));
        // Keep Mihomo's own filtering when a group includes dynamic nodes.
        let mut excluded = text(&m, "exclude-type")
            .split('|')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !excluded.iter().any(|s| s == "Direct") {
            excluded.push("Direct".into());
        }
        put(&mut m, "exclude-type", excluded.join("|"));
        let dynamic = ["include-all", "include-all-proxies", "include-all-providers"]
            .iter()
            .any(|k| m.get(*k).and_then(Value::as_bool) == Some(true));
        if seq(&m, "proxies").is_empty() && seq(&m, "use").is_empty() && !dynamic {
            put(&mut m, "proxies", string_values(vec!["REJECT".into()]));
        }
        put(&mut m, "empty-fallback", "REJECT");
        groups.push(Value::Mapping(m));
    }
    let gm: HashMap<_, _> = groups
        .iter()
        .filter_map(Value::as_mapping)
        .map(|g| (text(g, "name").to_owned(), g.clone()))
        .collect();
    for name in gm.keys() {
        reachable(name, &gm, &leaves, &providers, &mut HashSet::new())?;
    }
    let old_main_guards = strings(&seq(&old_guards, "rules"));
    let mut rules = strings(&seq(&config, "rules"))
        .into_iter()
        .filter(|r| !old_main_guards.contains(r))
        .collect::<Vec<_>>();
    // Mihomo skips proxies without UDP support. A final reject prevents its DIRECT fallback.
    // Explicit subscription rejects are promoted before CN rules, so only our guard can be last.
    if managed && rules.last().is_some_and(|r| r == "MATCH,REJECT") {
        rules.pop();
    }
    let mut fallback = None;
    for r in &rules {
        let p = tokens(r)?;
        if matches!(p[0].to_uppercase().as_str(), "MATCH" | "FINAL") {
            let n = &p[target_index(&p)];
            if !directs.contains(n)
                && !skipped.contains(n)
                && reachable(n, &gm, &leaves, &providers, &mut HashSet::new())?
            {
                fallback = Some(n.clone());
            }
            break;
        }
    }
    if fallback.is_none() {
        for g in &groups {
            let n = text(g.as_mapping().unwrap(), "name");
            if reachable(n, &gm, &leaves, &providers, &mut HashSet::new())? {
                fallback = Some(n.to_owned());
                break;
            }
        }
    }
    if fallback.is_none() {
        fallback = proxies
            .first()
            .and_then(Value::as_mapping)
            .map(|p| text(p, "name").to_owned());
    }
    if fallback.is_none() && !providers.is_empty() {
        let mut g = Mapping::new();
        put(&mut g, "name", FALLBACK);
        put(&mut g, "type", "select");
        put(
            &mut g,
            "use",
            string_values(providers.keys().filter_map(Value::as_str).map(str::to_owned).collect()),
        );
        put(&mut g, "empty-fallback", "REJECT");
        groups.push(Value::Mapping(g));
        fallback = Some(FALLBACK.into());
    }
    let Some(fallback) = fallback else {
        bail!("The subscription contains no proxy nodes or providers");
    };
    if fallback.contains(['&', '=']) {
        bail!("The default proxy name contains DNS selector delimiters; rename it before applying dual NIC policy");
    }
    report.proxy_dns_group = fallback.clone();
    report.proxy_count = proxies.len();
    report.provider_count = providers.len();
    let mut subrules = object(&config, "sub-rules");
    if managed {
        subrules.retain(|k, _| !k.as_str().is_some_and(|s| s.starts_with("__DualNic_Priority_")));
    }
    let old_sub_guards = object(&old_guards, "sub-rules");
    for (name, values) in &mut subrules {
        if let Some(guards) = old_sub_guards.get(name).and_then(Value::as_sequence) {
            if let Some(rules) = values.as_sequence_mut() {
                rules.retain(|r| !guards.contains(r));
            }
        }
    }
    validate_subrules(&subrules)?;
    if !managed
        && subrules
            .keys()
            .filter_map(Value::as_str)
            .any(|s| s.starts_with("__DualNic_Priority_"))
    {
        bail!("Reserved sub-rule name");
    }
    let rewrite = |r: &str, report: &mut Report, in_subrule: bool| -> Result<String> {
        let mut p = tokens(r)?;
        let ti = target_index(&p);
        let old = p[ti].clone();
        if p[0].eq_ignore_ascii_case("SUB-RULE") {
            if !subrules.contains_key(old.as_str()) {
                bail!("Unknown sub-rule: {old}");
            }
            return Ok(r.to_owned());
        }
        if !in_subrule
            && matches!(p[0].as_str(), "MATCH" | "FINAL")
            && (matches!(old.as_str(), "PASS" | "PASS-RULE")
                || skipped.contains(&old)
                || (gm.contains_key(&old) && !reachable(&old, &gm, &leaves, &providers, &mut HashSet::new())?))
        {
            p[ti] = fallback.clone();
        } else if skipped.contains(&old) {
            p[ti] = "REJECT".into();
        } else if directs.contains(&old) {
            if !local(r) {
                report.rewritten_direct_rules.push(r.to_owned());
                p[ti] = fallback.clone();
            }
        } else if old == UNMATCHED {
            p[ti] = if ctx.unmatched_ready() { UNMATCHED } else { "REJECT" }.into();
        } else if old == CN {
            p[ti] = if ctx.wired_ready() { CN } else { "REJECT" }.into();
        } else if old == USB {
            p[ti] = fallback.clone();
        } else if !matches!(old.as_str(), "REJECT" | "REJECT-DROP" | "PASS")
            && !leaves.contains(&old)
            && !gm.contains_key(&old)
            && old != FALLBACK
        {
            bail!("Unknown rule target: {old}");
        }
        if !ctx.proxy_ready()
            && !local(r)
            && p[ti] != CN
            && p[ti] != UNMATCHED
            && !matches!(p[ti].as_str(), "REJECT" | "REJECT-DROP")
        {
            p[ti] = "REJECT".into();
        }
        if !ctx.proxy_ready()
            && local(r)
            && !directs.contains(&p[ti])
            && !matches!(p[ti].as_str(), "REJECT" | "REJECT-DROP")
        {
            p[ti] = "REJECT".into();
        }
        Ok(p.join(","))
    };
    let mut first = Vec::new();
    let mut rest = Vec::new();
    let mut priority_seen = HashSet::new();
    for r in rules {
        let p = tokens(&r)?;
        let ti = target_index(&p);
        if matches!(p[0].to_uppercase().as_str(), "MATCH" | "FINAL") {
            continue;
        }
        if managed && p[0].eq_ignore_ascii_case("SUB-RULE") && p[ti].starts_with("__DualNic_Priority_") {
            let original = p[ti].trim_start_matches("__DualNic_Priority_");
            if !subrules.contains_key(original) {
                bail!("Unknown sub-rule: {original}");
            }
            if priority_seen.insert(r.clone()) {
                first.push(r);
            }
            continue;
        }
        if managed && p[ti] == CN && matches!(p[0].as_str(), "GEOSITE" | "GEOIP") && p.get(1).is_some_and(|v| v == "cn")
        {
            continue;
        }
        let rr = rewrite(&r, &mut report, false)?;
        if p[0].eq_ignore_ascii_case("SUB-RULE") {
            let priority = priority_rule(&rr)?;
            if priority_seen.insert(priority.clone()) {
                first.push(priority);
            }
            rest.push(rr);
            continue;
        }
        let q = tokens(&rr)?;
        if local(&rr)
            || (matches!(q[target_index(&q)].as_str(), "REJECT" | "REJECT-DROP")
                && matches!(p[ti].as_str(), "REJECT" | "REJECT-DROP"))
        {
            first.push(rr);
        } else {
            rest.push(rr);
        }
    }
    let cn_target = if ctx.wired_ready() { CN } else { "REJECT" };
    first.extend([format!("GEOSITE,cn,{cn_target}"), format!("GEOIP,cn,{cn_target}")]);
    first.extend(rest);
    if ctx.unmatched_ready() {
        first.push(format!("MATCH,{UNMATCHED}"));
    }
    first.push("MATCH,REJECT".into());
    let mut seen = HashSet::new();
    first.retain(|r| seen.insert(r.clone()));
    let proxy_names = leaves
        .iter()
        .cloned()
        .chain(gm.keys().cloned())
        .chain(std::iter::once(FALLBACK.into()))
        .collect();
    // Mihomo skips unsupported UDP proxies; a matching reject must precede the direct default.
    let (first, main_guards) = guard_proxy_rules(first, &proxy_names)?;
    put(&mut config, "rules", string_values(first));
    let mut new_sub = Mapping::new();
    for (name, values) in &subrules {
        let Some(v) = values.as_sequence() else {
            bail!("Invalid sub-rules: {name:?}");
        };
        let rs = strings(v)
            .into_iter()
            .map(|r| rewrite(&r, &mut report, true))
            .collect::<Result<Vec<_>>>()?;
        new_sub.insert(name.clone(), string_values(rs));
    }
    let originals = new_sub.clone();
    for (name, values) in &originals {
        if let Some(n) = name.as_str() {
            if !n.starts_with("__DualNic_Priority_") {
                let rs = strings(values.as_sequence().unwrap())
                    .into_iter()
                    .map(|r| priority_rule(&r))
                    .collect::<Result<Vec<_>>>()?;
                new_sub.insert(priority_name(n).into(), string_values(rs));
            }
        }
    }
    let mut sub_guards = Mapping::new();
    for (name, values) in &mut new_sub {
        if !name.as_str().is_some_and(|n| n.starts_with("__DualNic_Priority_")) {
            let (guarded, generated) = guard_proxy_rules(strings(values.as_sequence().unwrap()), &proxy_names)?;
            *values = string_values(guarded);
            sub_guards.insert(name.clone(), string_values(generated));
        }
    }
    put(&mut config, "sub-rules", new_sub);
    let mut guards = Mapping::new();
    put(&mut guards, "rules", string_values(main_guards));
    put(&mut guards, "sub-rules", sub_guards);
    put(&mut config, "x-dual-nic-udp-guards", guards);
    if ctx.proxy_ready() {
        if let Some(a) = ctx.proxy.as_ref() {
            proxies.push(helper(USB, a));
        }
    } else {
        proxies.clear();
        providers.clear();
        groups.clear();
    }
    if ctx.wired_ready() {
        if let Some(a) = ctx.wired.as_ref() {
            proxies.push(helper(CN, a));
        }
    }
    if let Some(a) = ctx.unmatched.as_ref().filter(|a| a.ready()) {
        proxies.push(helper(UNMATCHED, a));
    }
    proxies.extend(lan_directs);
    put(&mut config, "proxies", Value::Sequence(proxies));
    put(&mut config, "proxy-providers", providers);
    put(&mut config, "proxy-groups", Value::Sequence(groups));
    let cn_dns = vec![format!("223.5.5.5#{cn_target}"), format!("119.29.29.29#{cn_target}")];
    let usb_target = if ctx.proxy_ready() { USB } else { "REJECT" };
    let foreign_target = if ctx.proxy_ready() {
        fallback.as_str()
    } else if ctx.unmatched_ready() {
        UNMATCHED
    } else {
        "REJECT"
    };
    let unmatched_dns = vec![
        format!(
            "223.5.5.5#{}",
            if ctx.unmatched_ready() { UNMATCHED } else { cn_target }
        ),
        format!(
            "119.29.29.29#{}",
            if ctx.unmatched_ready() { UNMATCHED } else { cn_target }
        ),
    ];
    let foreign = if !ctx.proxy_ready() && ctx.unmatched_ready() {
        unmatched_dns.clone()
    } else {
        vec![
            format!("https://1.1.1.1/dns-query#{foreign_target}"),
            format!("https://1.0.0.1/dns-query#{foreign_target}"),
        ]
    };
    let mut dns = object(&config, "dns");
    put(&mut dns, "enable", true);
    if !ipv6_enabled
        || ![&ctx.wired, &ctx.proxy, &ctx.unmatched]
            .into_iter()
            .flatten()
            .any(|a| a.ready() && a.ipv6)
    {
        put(&mut dns, "ipv6", false);
    }
    put(&mut dns, "respect-rules", false);

    put(
        &mut dns,
        "default-nameserver",
        string_values(if ctx.wired_ready() {
            cn_dns.clone()
        } else {
            unmatched_dns.clone()
        }),
    );
    put(&mut dns, "direct-nameserver", string_values(unmatched_dns.clone()));
    put(&mut dns, "direct-nameserver-follow-policy", true);
    put(
        &mut dns,
        "proxy-server-nameserver",
        string_values(vec![
            format!("223.5.5.5#{usb_target}"),
            format!("119.29.29.29#{usb_target}"),
        ]),
    );
    put(&mut dns, "nameserver", string_values(foreign.clone()));
    let mut policy = object(&dns, "nameserver-policy");
    for (k, v) in policy.iter_mut() {
        let key = k.as_str().unwrap_or("");
        if !matches!(key, "geosite:private" | "+.lan" | "+.local" | "+.home.arpa") {
            *v = string_values(foreign.clone());
        }
    }
    put(&mut policy, "geosite:cn", string_values(cn_dns));
    put(&mut policy, "geosite:private", "system");
    put(&mut dns, "nameserver-policy", policy);
    for k in ["fallback", "fallback-filter", "proxy-server-nameserver-policy"] {
        dns.remove(k);
    }
    put(&mut config, "dns", dns);
    put(&mut config, "mode", "rule");
    config.remove("interface-name");
    config.remove("routing-mark");
    let mut tun = object(&config, "tun");
    put(&mut tun, "enable", true);
    put(&mut tun, "auto-route", true);
    // Unbound LAN DIRECT must avoid the TUN route; explicit public bindings take precedence.
    put(&mut tun, "auto-detect-interface", true);
    put(&mut tun, "strict-route", true);
    put(&mut tun, "device", "ClashDualNic");
    put(
        &mut tun,
        "dns-hijack",
        string_values(vec!["any:53".into(), "tcp://any:53".into()]),
    );
    for k in [
        "route-address",
        "route-address-set",
        "route-exclude-address",
        "route-exclude-address-set",
        "inet4-route-address",
        "inet6-route-address",
        "inet4-route-exclude-address",
        "inet6-route-exclude-address",
        "include-interface",
        "exclude-interface",
        "exclude-src-port",
        "exclude-src-port-range",
        "exclude-dst-port-range",
        "include-mac-address",
        "exclude-mac-address",
        "exclude-dst-port",
        "include-uid",
        "include-uid-range",
        "exclude-uid",
        "exclude-uid-range",
        "include-android-user",
        "include-package",
        "exclude-package",
    ] {
        tun.remove(k);
    }
    if ipv6_enabled
        && [&ctx.wired, &ctx.proxy, &ctx.unmatched]
            .into_iter()
            .flatten()
            .any(|a| a.ready() && a.ipv6)
    {
        put(
            &mut tun,
            "inet6-address",
            string_values(vec!["fdfe:dcba:9876::1/126".into()]),
        );
    } else {
        tun.remove("inet6-address");
    }
    put(&mut config, "tun", tun);
    put(&mut config, "x-dual-nic-version", 2_u64);
    Ok((config, report))
}

#[derive(Debug, serde::Serialize)]
pub struct AdapterCounters {
    #[serde(flatten)]
    pub adapter: Adapter,
    pub physical: bool,
    pub interface_id: String,
    pub received: String,
    pub sent: String,
}
