//! Timed tests across the classes in maintenance.toml.

use maintenance_macros::timed_test;
use std::time::Duration;

#[timed_test(class = "integration")]
fn login_round_trip() {
    std::thread::sleep(Duration::from_millis(40));
    assert!(api::login("correct horse battery"));
}

#[timed_test]
fn admin_report_counts() -> Result<(), String> {
    if api::admin_report() == 2 { Ok(()) } else { Err("count".into()) }
}

// this one is over its threshold on purpose: the clock fails it with MAINT2050, which is what it expects
#[timed_test(class = "tight")]
#[should_panic(expected = "MAINT2050")]
fn over_its_threshold() {
    std::thread::sleep(Duration::from_millis(20));
}

#[timed_test(max = "500ms")]
#[tokio::test]
async fn async_status() {
    tokio::time::sleep(Duration::from_millis(15)).await;
    assert_eq!(api::status(), "vault 7");
}
