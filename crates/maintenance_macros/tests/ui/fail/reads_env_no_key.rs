use maintenance_macros::reads_env;

#[reads_env(DATABASE_URL)]
pub fn database_url() {}

fn main() {}
