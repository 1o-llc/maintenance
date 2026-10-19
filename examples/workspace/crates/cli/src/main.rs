//! A command-line tool the vault's boundary does not allow.

fn main() {
    let lines = vault::audit_log();
    println!("{} audit lines", lines.len());
}
