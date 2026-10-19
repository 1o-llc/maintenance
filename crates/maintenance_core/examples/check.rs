//! Every check against a workspace, printed: `cargo run -p maintenance_core --features check --example check -- <workspace>`.
//! Writes the workspace's registry and prints its timing report too. The example workspace is `examples/workspace`.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "examples/workspace".into()));
    let ws = maintenance_core::check::Workspace::scan(&root)?;
    let reports = ws.all();
    for report in &reports {
        print!("{}", report.render());
    }
    let written = ws.write_registry()?;
    let registry = maintenance_core::registry::read(&root)?;
    println!("registry: {} ({} items; {} at secret or above)", written.display(), registry.items.len(), registry.at_or_above("secret").count());
    match maintenance_core::timed::report(&root) {
        Ok(timing) => print!("{}", timing.render(10)),
        Err(e) => println!("no timing report under {}: {e}", root.display()),
    }
    if reports.iter().any(|r| !r.passed) {
        std::process::exit(1);
    }
    Ok(())
}
