use clash_verge_dualnic::{Adapter, CN, Context, USB, transform};
use serde_yaml_ng::{Mapping, Value};
fn adapter(name: &str) -> Adapter {
    Adapter {
        guid: name.into(),
        name: name.into(),
        description: name.into(),
        up: true,
        addresses: vec!["192.168.1.2".into()],
        gateways: vec!["192.168.1.1".into()],
        ipv4: true,
        ipv6: false,
    }
}
fn context() -> Context {
    Context {
        wired: Some(adapter("以太网")),
        proxy: Some(adapter("USB 网卡")),
        ..Default::default()
    }
}
fn input() -> Mapping {
    serde_yaml_ng::from_str("proxies:\n  - {name: node, type: ss, server: 203.0.113.2, port: 443, cipher: aes-128-gcm, password: test}\nproxy-groups:\n  - {name: Main, type: select, proxies: [DIRECT, node]}\n  - {name: Media, type: select, proxies: [Main]}\nrules:\n  - IP-CIDR,192.168.0.0/16,DIRECT,no-resolve\n  - DOMAIN,blocked.example,REJECT\n  - DOMAIN,media.example,Media\n  - DOMAIN,direct.example,DIRECT\n  - MATCH,Main\ndns:\n  fake-ip-filter: [\"+.lan\"]\n").unwrap()
}
#[test]
fn binds_nodes_and_keeps_classification() {
    let (c, r) = transform(input(), &context()).unwrap();
    assert_eq!(r.proxy_dns_group, "Main");
    assert_eq!(c["proxies"][0]["interface-name"], Value::from("USB 网卡"));
    let rules = c["rules"].as_sequence().unwrap();
    assert_eq!(rules[2], Value::from(format!("GEOSITE,cn,{CN}")));
    assert!(rules.contains(&Value::from("DOMAIN,media.example,Media")));
    assert!(rules.contains(&Value::from("DOMAIN,direct.example,Main")));
    assert_eq!(
        c["proxy-groups"][0]["proxies"],
        serde_yaml_ng::from_str::<Value>("[node]").unwrap()
    );
}
#[test]
fn dns_uses_explicit_outputs() {
    let (c, _) = transform(input(), &context()).unwrap();
    assert_eq!(
        c["dns"]["proxy-server-nameserver"][0],
        Value::from(format!("223.5.5.5#{USB}"))
    );
    assert_eq!(c["dns"]["direct-nameserver"][0], Value::from(format!("223.5.5.5#{CN}")));
    assert_eq!(c["dns"]["fake-ip-filter"][0], Value::from("+.lan"));
    assert_eq!(c["tun"]["auto-detect-interface"], Value::Bool(true));
}
#[test]
fn usb_disconnection_has_no_proxy_transports() {
    let mut ctx = context();
    ctx.proxy = None;
    let (c, _) = transform(input(), &ctx).unwrap();
    assert!(c["proxy-groups"].as_sequence().unwrap().is_empty());
    assert!(c["proxy-providers"].as_mapping().unwrap().is_empty());
    assert_eq!(c["proxies"].as_sequence().unwrap().len(), 1);
    assert!(c["rules"].as_sequence().unwrap().contains(&Value::from("MATCH,REJECT")));
    assert_eq!(c["dns"]["proxy-server-nameserver"][0], Value::from("223.5.5.5#REJECT"));
}
#[test]
fn wired_disconnection_does_not_disable_usb() {
    let mut ctx = context();
    ctx.wired = None;
    let (c, _) = transform(input(), &ctx).unwrap();
    assert_eq!(c["proxies"][0]["interface-name"], Value::from("USB 网卡"));
    assert!(
        c["rules"]
            .as_sequence()
            .unwrap()
            .contains(&Value::from("GEOSITE,cn,REJECT"))
    );
}
#[test]
fn no_nodes_rejects_generation() {
    let mut c = input();
    c.remove("proxies");
    assert!(transform(c, &context()).is_err());
}
#[test]
fn cycles_reject_generation() {
    let mut c = input();
    c.insert(
        "proxy-groups".into(),
        serde_yaml_ng::from_str(
            "[{name: Main, type: select, proxies: [Media]}, {name: Media, type: select, proxies: [Main]}]",
        )
        .unwrap(),
    );
    assert!(transform(c, &context()).is_err());
}
#[test]
fn provider_override_and_dynamic_groups_are_bound() {
    let mut c = input();
    c.remove("proxies");
    c.insert(
        "proxy-providers".into(),
        serde_yaml_ng::from_str(
            "{airport: {type: http, url: 'https://example.com/proxies', path: providers/airport.yaml}}",
        )
        .unwrap(),
    );
    c.insert(
        "proxy-groups".into(),
        serde_yaml_ng::from_str("[{name: Main, type: select, include-all-providers: true}]").unwrap(),
    );
    c.insert("rules".into(), Value::Sequence(vec![Value::from("MATCH,Main")]));
    let (c, _) = transform(c, &context()).unwrap();
    assert_eq!(
        c["proxy-providers"]["airport"]["override"]["interface-name"],
        Value::from("USB 网卡")
    );
}
#[test]
fn expressions_and_chains_are_rejected() {
    let mut c = input();
    c["proxies"][0]["dialer-proxy"] = Value::from("Main");
    assert!(transform(c, &context()).is_err());
    let mut c = input();
    c.insert(
        "proxy-providers".into(),
        serde_yaml_ng::from_str("{airport: {override-expr: 'proxy.interface = x'}}").unwrap(),
    );
    assert!(transform(c, &context()).is_err());
}
#[test]
fn public_route_exclusions_are_removed() {
    let mut c = input();
    c.insert(
        "tun".into(),
        serde_yaml_ng::from_str("{route-exclude-address: [8.8.8.8/32], auto-route: false}").unwrap(),
    );
    let (c, _) = transform(c, &context()).unwrap();
    assert!(c["tun"].get("route-exclude-address").is_none());
    assert_eq!(c["tun"]["auto-route"], Value::Bool(true));
}
#[test]
fn repeated_generation_is_idempotent() {
    let (c, _) = transform(input(), &context()).unwrap();
    let (d, _) = transform(c.clone(), &context()).unwrap();
    assert_eq!(c, d);
}
#[test]
fn loopback_placeholders_are_not_real_nodes() {
    let mut c = input();
    c["proxies"]
        .as_sequence_mut()
        .unwrap()
        .push(serde_yaml_ng::from_str("{name: status, type: ss, server: 127.0.0.1, port: 1}").unwrap());
    let (c, r) = transform(c, &context()).unwrap();
    assert_eq!(r.skipped_nodes, vec!["status"]);
    assert!(
        !c["proxies"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|p| p["name"] == Value::from("status"))
    );
}
#[test]
fn no_ipv6_output_is_implicitly_enabled() {
    let mut c = input();
    c.insert("ipv6".into(), Value::Bool(false));
    let mut ctx = context();
    ctx.proxy.as_mut().unwrap().ipv6 = true;
    let (c, _) = transform(c, &ctx).unwrap();
    assert_eq!(c["ipv6"], Value::Bool(false));
    assert!(c["tun"].get("inet6-address").is_none());
}

#[test]
fn subrule_cycles_are_rejected() {
    let mut c = input();
    c.insert(
        "sub-rules".into(),
        serde_yaml_ng::from_str("{one: ['SUB-RULE,(NETWORK,TCP),two'], two: ['SUB-RULE,(NETWORK,TCP),one']}").unwrap(),
    );
    assert!(transform(c, &context()).is_err());
}
#[test]
fn provider_post_override_is_rejected() {
    let mut c = input();
    c.insert(
        "proxy-providers".into(),
        serde_yaml_ng::from_str("{airport: {override: {override-expr: ['.mptcp = true']}}}").unwrap(),
    );
    assert!(transform(c, &context()).is_err());
}
#[test]
fn named_lan_direct_has_a_definition() {
    let mut c = input();
    c["proxies"]
        .as_sequence_mut()
        .unwrap()
        .push(serde_yaml_ng::from_str("{name: LAN, type: direct, interface-name: LAN Adapter}").unwrap());
    c["rules"]
        .as_sequence_mut()
        .unwrap()
        .insert(0, Value::from("IP-CIDR,10.0.0.0/8,LAN"));
    let (c, _) = transform(c, &context()).unwrap();
    assert!(
        c["proxies"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|p| p["name"] == Value::from("LAN"))
    );
}
#[test]
fn dynamic_group_keeps_native_filters() {
    let mut c = input();
    c["proxy-groups"][0]["include-all"] = Value::Bool(true);
    c["proxy-groups"][0]["filter"] = Value::from("node");
    let (c, _) = transform(c, &context()).unwrap();
    assert_eq!(c["proxy-groups"][0]["filter"], Value::from("node"));
    assert_eq!(c["proxy-groups"][0]["exclude-type"], Value::from("Direct"));
}
#[test]
fn nested_rejects_get_a_priority_pass() {
    let mut c = input();
    c.insert(
        "sub-rules".into(),
        serde_yaml_ng::from_str("{one: ['DOMAIN,blocked.cn,REJECT', 'MATCH,Main']}").unwrap(),
    );
    c["rules"]
        .as_sequence_mut()
        .unwrap()
        .insert(0, Value::from("SUB-RULE,(NETWORK,TCP),one"));
    let (c, _) = transform(c, &context()).unwrap();
    assert_eq!(
        c["rules"][0],
        Value::from("SUB-RULE,(NETWORK,TCP),__DualNic_Priority_one")
    );
    assert_eq!(
        c["sub-rules"]["__DualNic_Priority_one"][1],
        Value::from("MATCH,PASS-RULE")
    );
}

#[test]
fn nested_policy_is_idempotent() {
    let mut c = input();
    c.insert(
        "sub-rules".into(),
        serde_yaml_ng::from_str("{one: ['DOMAIN,blocked.cn,REJECT', 'MATCH,Main']}").unwrap(),
    );
    c["rules"]
        .as_sequence_mut()
        .unwrap()
        .insert(0, Value::from("SUB-RULE,(NETWORK,TCP),one"));
    let (c, _) = transform(c, &context()).unwrap();
    let (d, _) = transform(c.clone(), &context()).unwrap();
    assert_eq!(c, d);
}
#[test]
fn provider_policy_is_idempotent() {
    let mut c = input();
    c.insert(
        "proxy-providers".into(),
        serde_yaml_ng::from_str("{airport: {type: http, url: 'https://example.com'}}").unwrap(),
    );
    let (c, _) = transform(c, &context()).unwrap();
    let (d, _) = transform(c.clone(), &context()).unwrap();
    assert_eq!(c, d);
}

#[test]
fn logical_lan_constraints_are_preserved() {
    let mut c = input();
    c["rules"]
        .as_sequence_mut()
        .unwrap()
        .insert(0, Value::from("AND,((NETWORK,TCP),(IP-CIDR,10.0.0.0/8)),DIRECT"));
    c["rules"].as_sequence_mut().unwrap().insert(
        1,
        Value::from("OR,((IP-CIDR,10.0.0.0/8),(IP-CIDR,192.168.0.0/16)),DIRECT"),
    );
    c["rules"].as_sequence_mut().unwrap().insert(
        2,
        Value::from("OR,((IP-CIDR,10.0.0.0/8),(DOMAIN,public.example)),DIRECT"),
    );
    let (c, r) = transform(c, &context()).unwrap();
    assert_eq!(
        c["rules"][0],
        Value::from("AND,((NETWORK,TCP),(IP-CIDR,10.0.0.0/8)),DIRECT")
    );
    assert_eq!(
        c["rules"][1],
        Value::from("OR,((IP-CIDR,10.0.0.0/8),(IP-CIDR,192.168.0.0/16)),DIRECT")
    );
    assert!(r.rewritten_direct_rules.iter().any(|r| r.contains("public.example")));
}
#[test]
fn disabled_ipv6_controls_nodes_providers_and_dns() {
    let mut c = input();
    c.insert("ipv6".into(), Value::Bool(false));
    c["dns"]["ipv6"] = Value::Bool(true);
    c.insert(
        "proxy-providers".into(),
        serde_yaml_ng::from_str("{airport: {type: http, url: 'https://example.com'}}").unwrap(),
    );
    let mut ctx = context();
    ctx.proxy.as_mut().unwrap().ipv6 = true;
    let (c, _) = transform(c, &ctx).unwrap();
    assert_eq!(c["proxies"][0]["ip-version"], Value::from("ipv4"));
    assert_eq!(
        c["proxy-providers"]["airport"]["override"]["ip-version"],
        Value::from("ipv4")
    );
    assert_eq!(c["dns"]["ipv6"], Value::Bool(false));
}
#[test]
fn port_exclusions_cannot_bypass_the_tun() {
    let mut c = input();
    c.insert(
        "tun".into(),
        serde_yaml_ng::from_str("{exclude-src-port: [12345], exclude-dst-port: [443], include-interface: [USB]}")
            .unwrap(),
    );
    let (c, _) = transform(c, &context()).unwrap();
    for key in ["exclude-src-port", "exclude-dst-port", "include-interface"] {
        assert!(c["tun"].get(key).is_none());
    }
}

#[test]
fn additional_rule_parameters_keep_the_target_position() {
    let mut c = input();
    c["rules"]
        .as_sequence_mut()
        .unwrap()
        .insert(0, Value::from("GEOIP,cn,DIRECT,resolve"));
    c["rules"]
        .as_sequence_mut()
        .unwrap()
        .insert(1, Value::from("IP-CIDR,10.0.0.0/8,DIRECT,no-resolve,src"));
    let (c, r) = transform(c, &context()).unwrap();
    assert!(
        c["rules"]
            .as_sequence()
            .unwrap()
            .contains(&Value::from("GEOIP,cn,Main,resolve"))
    );
    assert!(
        c["rules"]
            .as_sequence()
            .unwrap()
            .contains(&Value::from("IP-CIDR,10.0.0.0/8,Main,no-resolve,src"))
    );
    assert!(r.rewritten_direct_rules.iter().any(|r| r.ends_with("src")));
}

#[test]
fn terminal_reject_is_managed_after_domestic_rules() {
    let mut c = input();
    let r = c["rules"].as_sequence_mut().unwrap();
    *r.last_mut().unwrap() = Value::from("MATCH,REJECT");
    let (c, _) = transform(c, &context()).unwrap();
    let r = c["rules"].as_sequence().unwrap();
    let reject = r.iter().position(|r| r.as_str() == Some("MATCH,REJECT")).unwrap();
    let cn = r
        .iter()
        .position(|r| r.as_str() == Some("GEOSITE,cn,__DualNic_CN"))
        .unwrap();
    assert!(reject > cn);
}

#[test]
fn terminal_pass_cannot_fall_through_to_unbound_direct() {
    let mut c = input();
    *c["rules"].as_sequence_mut().unwrap().last_mut().unwrap() = Value::from("MATCH,PASS");
    let (c, report) = transform(c, &context()).unwrap();
    assert_eq!(report.proxy_dns_group, "Main");
    assert!(c["rules"].as_sequence().unwrap().contains(&Value::from("MATCH,REJECT")));
    assert!(!c["rules"].as_sequence().unwrap().contains(&Value::from("MATCH,PASS")));
}

#[test]
fn nested_priority_keeps_scanning_after_classification_rules() {
    let mut c = input();
    c.insert(
        "sub-rules".into(),
        serde_yaml_ng::from_str(
            "{one: ['DOMAIN-SUFFIX,cn,Main', 'DOMAIN,blocked.cn,REJECT', 'IP-CIDR,10.0.0.0/8,DIRECT', 'MATCH,Main']}",
        )
        .unwrap(),
    );
    c["rules"]
        .as_sequence_mut()
        .unwrap()
        .insert(0, Value::from("SUB-RULE,(NETWORK,TCP),one"));
    let (c, _) = transform(c, &context()).unwrap();
    let priority = c["sub-rules"]["__DualNic_Priority_one"].as_sequence().unwrap();
    assert_eq!(priority[0], Value::from("DOMAIN-SUFFIX,cn,PASS-RULE"));
    assert_eq!(priority[1], Value::from("DOMAIN,blocked.cn,REJECT"));
    assert_eq!(priority[2], Value::from("IP-CIDR,10.0.0.0/8,DIRECT"));
}

#[test]
fn proxy_group_cannot_choose_a_rule_skipping_exit() {
    let mut c = input();
    c["proxy-groups"].as_sequence_mut().unwrap()[0]["proxies"] =
        serde_yaml_ng::from_str("[DIRECT, PASS, PASS-RULE, REJECT, node]").unwrap();
    let (c, _) = transform(c, &context()).unwrap();
    assert_eq!(
        c["proxy-groups"][0]["proxies"],
        serde_yaml_ng::from_str::<Value>("[REJECT, node]").unwrap()
    );
}

#[test]
fn nested_terminal_pass_preserves_later_category_rules() {
    let mut c = input();
    c.insert(
        "sub-rules".into(),
        serde_yaml_ng::from_str("{one: ['DOMAIN,blocked.example,REJECT', 'MATCH,PASS']}").unwrap(),
    );
    c["rules"]
        .as_sequence_mut()
        .unwrap()
        .insert(0, Value::from("SUB-RULE,(NETWORK,TCP),one"));
    let (c, _) = transform(c, &context()).unwrap();
    assert_eq!(c["sub-rules"]["one"][1], Value::from("MATCH,PASS"));
    assert!(
        c["rules"]
            .as_sequence()
            .unwrap()
            .contains(&Value::from("DOMAIN,media.example,Media"))
    );
    assert!(c["rules"].as_sequence().unwrap().contains(&Value::from("MATCH,REJECT")));
}

#[test]
fn unsupported_udp_has_a_final_reject_after_classification() {
    let mut c = input();
    c["proxies"].as_sequence_mut().unwrap()[0]["udp"] = Value::Bool(false);
    let (c, _) = transform(c, &context()).unwrap();
    let rules = c["rules"].as_sequence().unwrap();
    assert_eq!(rules.last(), Some(&Value::from("MATCH,REJECT")));
    assert!(
        rules
            .iter()
            .position(|r| r.as_str() == Some("DOMAIN,media.example,Media"))
            .unwrap()
            < rules.len() - 1
    );
}

#[test]
fn repeated_policy_does_not_promote_the_generated_udp_guard() {
    let (c, _) = transform(input(), &context()).unwrap();
    let (d, _) = transform(c.clone(), &context()).unwrap();
    assert_eq!(c, d);
    assert_eq!(
        d["rules"].as_sequence().unwrap().last(),
        Some(&Value::from("MATCH,REJECT"))
    );
}

#[test]
fn repeated_policy_keeps_the_managed_terminal_reject() {
    let mut c = input();
    *c["rules"].as_sequence_mut().unwrap().last_mut().unwrap() = Value::from("MATCH,REJECT");
    let (c, _) = transform(c, &context()).unwrap();
    let (d, _) = transform(c.clone(), &context()).unwrap();
    assert_eq!(c, d);
    let rules = d["rules"].as_sequence().unwrap();
    assert!(
        rules.iter().position(|r| r.as_str() == Some("MATCH,REJECT")).unwrap()
            > rules
                .iter()
                .position(|r| r.as_str().is_some_and(|s| s.starts_with("GEOSITE,cn,")))
                .unwrap()
    );
}

#[test]
fn an_empty_terminal_group_uses_the_detected_default_exit() {
    let mut c = input();
    c["proxy-groups"]
        .as_sequence_mut()
        .unwrap()
        .push(serde_yaml_ng::from_str("{name: Empty, type: select, proxies: []}").unwrap());
    *c["rules"].as_sequence_mut().unwrap().last_mut().unwrap() = Value::from("MATCH,Empty");
    let (c, report) = transform(c, &context()).unwrap();
    assert_eq!(report.proxy_dns_group, "Main");
    assert!(c["rules"].as_sequence().unwrap().contains(&Value::from("MATCH,REJECT")));
    assert!(!c["rules"].as_sequence().unwrap().contains(&Value::from("MATCH,Empty")));
}

#[test]
fn a_terminal_status_node_uses_a_real_default_exit() {
    let mut c = input();
    c["proxies"].as_sequence_mut().unwrap().push(
        serde_yaml_ng::from_str(
            "{name: status, type: ss, server: 127.0.0.1, port: 1, cipher: aes-128-gcm, password: test}",
        )
        .unwrap(),
    );
    *c["rules"].as_sequence_mut().unwrap().last_mut().unwrap() = Value::from("MATCH,status");
    let (c, report) = transform(c, &context()).unwrap();
    assert_eq!(report.proxy_dns_group, "Main");
    assert_eq!(report.skipped_nodes, vec!["status"]);
    assert!(c["rules"].as_sequence().unwrap().contains(&Value::from("MATCH,REJECT")));
}

#[test]
fn lan_uses_physical_routes_without_relaxing_public_bindings() {
    let (config, _) = transform(input(), &context()).unwrap();
    assert_eq!(
        config["rules"][0],
        Value::from("IP-CIDR,192.168.0.0/16,DIRECT,no-resolve")
    );
    assert_eq!(config["tun"]["auto-detect-interface"], Value::Bool(true));
    assert_eq!(config["tun"]["strict-route"], Value::Bool(true));
    let proxies = config["proxies"].as_sequence().unwrap();
    assert_eq!(
        proxies.iter().find(|p| p["name"] == "node").unwrap()["interface-name"],
        Value::from("USB 网卡")
    );
    assert_eq!(
        proxies.iter().find(|p| p["name"] == CN).unwrap()["interface-name"],
        Value::from("以太网")
    );
    assert!(
        !config["proxy-groups"][0]["proxies"]
            .as_sequence()
            .unwrap()
            .contains(&Value::from("DIRECT"))
    );
    assert_eq!(
        config["rules"].as_sequence().unwrap().last().unwrap(),
        &Value::from("MATCH,REJECT")
    );
}

#[test]
fn configured_backups_are_used_and_primary_recovery_wins() {
    use clash_verge_dualnic::Settings;
    let wired = adapter("wired");
    let proxy = adapter("proxy");
    let backup = adapter("backup");
    let settings = Settings {
        enabled: true,
        wired_guid: wired.guid.clone(),
        proxy_guid: proxy.guid.clone(),
        wired_backup_guid: Some(backup.guid.clone()),
        proxy_backup_guid: Some(backup.guid.clone()),
        ..Default::default()
    };
    let ctx = Context::resolve(&settings, &[backup.clone()]).unwrap();
    assert_eq!(ctx.routes().domestic.state, "backup");
    assert_eq!(ctx.routes().proxy.state, "backup");
    let (config, _) = transform(input(), &ctx).unwrap();
    let proxies = config["proxies"].as_sequence().unwrap();
    for name in [CN, "node"] {
        assert_eq!(
            proxies.iter().find(|p| p["name"] == name).unwrap()["interface-name"],
            Value::from("backup")
        );
    }
    let ctx = Context::resolve(&settings, &[wired, proxy, backup]).unwrap();
    assert_eq!(ctx.routes().domestic.state, "primary");
    assert_eq!(ctx.routes().proxy.state, "primary");
}
#[test]
fn all_roles_can_share_an_adapter_without_extra_backup_capacity() {
    use clash_verge_dualnic::{Settings, UNMATCHED};
    let adapter = adapter("shared");
    let settings = Settings {
        wired_guid: "shared".into(),
        proxy_guid: "shared".into(),
        wired_backup_guid: Some("shared".into()),
        proxy_backup_guid: Some("shared".into()),
        unmatched_guid: Some("shared".into()),
        ..Default::default()
    };
    let ctx = Context::resolve(&settings, &[adapter.clone()]).unwrap();
    assert_eq!(ctx.routes().domestic.state, "primary");
    let (config, _) = transform(input(), &ctx).unwrap();
    assert!(
        config["proxies"]
            .as_sequence()
            .unwrap()
            .iter()
            .all(|p| p["interface-name"] == "shared")
    );
    assert!(
        config["rules"]
            .as_sequence()
            .unwrap()
            .contains(&Value::from(format!("MATCH,{UNMATCHED}")))
    );
    let mut down = adapter;
    down.up = false;
    let ctx = Context::resolve(&settings, &[down]).unwrap();
    assert!(!ctx.wired_ready() && !ctx.proxy_ready() && !ctx.unmatched_ready());
}
#[test]
fn unmatched_default_is_explicit_and_does_not_replace_classification() {
    use clash_verge_dualnic::UNMATCHED;
    let mut ctx = context();
    ctx.unmatched = Some(adapter("default"));
    let (config, report) = transform(input(), &ctx).unwrap();
    assert_eq!(report.proxy_dns_group, "Main");
    let rules = config["rules"].as_sequence().unwrap();
    assert!(rules.contains(&Value::from("DOMAIN,media.example,Media")));
    assert!(rules.contains(&Value::from("DOMAIN,blocked.example,REJECT")));
    assert!(rules.contains(&Value::from(format!("MATCH,{UNMATCHED}"))));
    assert!(!rules.contains(&Value::from("MATCH,Main")));
    let (repeated, _) = transform(config.clone(), &ctx).unwrap();
    assert_eq!(config, repeated);
    ctx.unmatched.as_mut().unwrap().up = false;
    let (config, _) = transform(input(), &ctx).unwrap();
    assert_eq!(
        config["rules"].as_sequence().unwrap().last().unwrap(),
        &Value::from("MATCH,REJECT")
    );
}
#[test]
fn default_dns_survives_without_domestic_or_proxy_outputs() {
    let ctx = Context {
        unmatched: Some(adapter("default")),
        ..Default::default()
    };
    let (config, _) = transform(input(), &ctx).unwrap();
    assert_eq!(
        config["dns"]["nameserver"][0],
        Value::from("223.5.5.5#__DualNic_Unmatched")
    );
    assert_eq!(
        config["dns"]["direct-nameserver"][0],
        Value::from("223.5.5.5#__DualNic_Unmatched")
    );
    assert_eq!(config["dns"]["direct-nameserver-follow-policy"], Value::Bool(true));
    assert_eq!(
        config["dns"]["nameserver-policy"]["geosite:cn"][0],
        Value::from("223.5.5.5#REJECT")
    );
    assert_eq!(
        config["dns"]["proxy-server-nameserver"][0],
        Value::from("223.5.5.5#REJECT")
    );
    assert!(
        config["rules"]
            .as_sequence()
            .unwrap()
            .contains(&Value::from("DOMAIN,media.example,REJECT"))
    );
}
#[test]
fn final_is_overridden_but_nested_match_keeps_its_meaning() {
    let mut c = input();
    *c["rules"].as_sequence_mut().unwrap().last_mut().unwrap() = Value::from("FINAL,Main");
    c.insert(
        "sub-rules".into(),
        serde_yaml_ng::from_str("{nested: ['MATCH,Main']}").unwrap(),
    );
    let (config, _) = transform(c, &context()).unwrap();
    assert!(
        !config["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|r| r.as_str().unwrap().starts_with("FINAL,"))
    );
    assert_eq!(config["sub-rules"]["nested"][0], Value::from("MATCH,Main"));
}
#[test]
fn old_settings_have_no_implicit_backup_and_unknown_versions_are_rejected() {
    use clash_verge_dualnic::Settings;
    let settings: Settings =
        serde_json::from_str(r#"{"version":1,"wired_guid":"wired","proxy_guid":"wired"}"#).unwrap();
    assert!(
        settings.wired_backup_guid.is_none()
            && settings.proxy_backup_guid.is_none()
            && settings.unmatched_guid.is_none()
    );
    assert!(Context::resolve(&settings, &[adapter("wired")]).is_ok());
    let invalid = Settings { version: 3, ..settings };
    assert!(Context::resolve(&invalid, &[]).is_err());
}

#[test]
fn matched_proxy_rules_cannot_skip_udp_into_the_direct_default() {
    let mut ctx = context();
    ctx.unmatched = Some(adapter("default"));
    let (config, _) = transform(input(), &ctx).unwrap();
    let rules = config["rules"].as_sequence().unwrap();
    let index = rules
        .iter()
        .position(|r| r.as_str() == Some("DOMAIN,media.example,Media"))
        .unwrap();
    assert_eq!(rules[index + 1], Value::from("DOMAIN,media.example,REJECT"));
    let (repeated, _) = transform(config.clone(), &ctx).unwrap();
    assert_eq!(config, repeated);
}

#[test]
fn disabling_ipv6_selects_an_ipv4_backup_instead_of_an_ipv6_only_primary() {
    let mut ctx = context();
    ctx.wired.as_mut().unwrap().ipv4 = false;
    ctx.wired.as_mut().unwrap().ipv6 = true;
    ctx.wired_backup = Some(adapter("v4backup"));
    let mut config = input();
    config.insert("ipv6".into(), Value::Bool(false));
    let (config, report) = transform(config, &ctx).unwrap();
    assert_eq!(report.routes.domestic.state, "backup");
    assert_eq!(
        config["proxies"]
            .as_sequence()
            .unwrap()
            .iter()
            .find(|p| p["name"] == CN)
            .unwrap()["interface-name"],
        Value::from("v4backup")
    );
}
