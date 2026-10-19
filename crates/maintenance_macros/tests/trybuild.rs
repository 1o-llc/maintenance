//! Every refusal, as the compiler shows it, and every macro compiling clean.

#[test]
fn ui_compile_fail_matrix() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/fail/*.rs");
}

#[test]
fn ui_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/*.rs");
}
