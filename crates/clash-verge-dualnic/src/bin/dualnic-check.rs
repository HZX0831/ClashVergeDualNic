use anyhow::{Context as _, Result};
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "adapters") {
        #[cfg(windows)]
        println!(
            "{}",
            serde_json::to_string_pretty(&clash_verge_dualnic::windows::adapters()?)?
        );
        return Ok(());
    }
    if args.get(1).is_some_and(|s| s == "traffic") {
        #[cfg(windows)]
        println!(
            "{}",
            serde_json::to_string_pretty(&clash_verge_dualnic::windows::network_counters()?)?
        );
        return Ok(());
    }
    if args.get(1).is_some_and(|s| s == "tcp") {
        #[cfg(windows)]
        println!(
            "{}",
            serde_json::to_string_pretty(&clash_verge_dualnic::windows::tcp_sockets(
                args.get(2).context("Missing owned PID")?.parse()?
            )?)?
        );
        return Ok(());
    }
    let config_path = args
        .get(1)
        .context("Usage: dualnic-check input.yaml context.json output.yaml")?;
    let ctx_path = args.get(2).context("Missing context JSON")?;
    let output = args.get(3).context("Missing output YAML")?;
    let config = serde_yaml_ng::from_str(&std::fs::read_to_string(config_path)?)?;
    let ctx: clash_verge_dualnic::Context = serde_json::from_str(&std::fs::read_to_string(ctx_path)?)?;
    let (config, report) = clash_verge_dualnic::transform(config, &ctx)?;
    std::fs::write(output, serde_yaml_ng::to_string(&config)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
