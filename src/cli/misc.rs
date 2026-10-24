use super::*;

pub(super) fn cmd_generate_shell_completion(sub: &ArgMatches) -> Result<()> {
    let raw = sub
        .get_one::<String>("shell")
        .context("missing required <shell>")?;
    let Ok(generator) = raw.parse::<clap_complete::shells::Shell>() else {
        bail!("unsupported completion shell `{raw}`");
    };
    let mut cmd = cli();
    clap_complete::generate(generator, &mut cmd, "mamba", &mut std::io::stdout());
    Ok(())
}

pub(super) fn cmd_surface_report(sub: &ArgMatches) -> Result<()> {
    let package = sub
        .get_one::<String>("package")
        .expect("clap enforces required");
    let typeshed = sub
        .get_one::<String>("typeshed")
        .cloned()
        .unwrap_or_else(|| {
            // Default: probe project-local checkout, fall back to a repo-root one.
            for p in ["vendor/typeshed", "vendor/typeshed"] {
                if std::path::Path::new(p).is_dir() {
                    return p.to_string();
                }
            }
            "vendor/typeshed".to_string()
        });
    let mamba_src = sub
        .get_one::<String>("mamba-src")
        .cloned()
        .unwrap_or_else(|| "src".to_string());

    let report = mamba::surface::build_report(
        package,
        std::path::Path::new(&typeshed),
        std::path::Path::new(&mamba_src),
    )
    .map_err(|e| anyhow::anyhow!(e))
    .with_context(|| format!("surface report for `{}`", package))?;

    print!("{}", report.render());
    Ok(())
}
