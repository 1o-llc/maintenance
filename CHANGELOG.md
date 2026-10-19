# Changelog

## 0.1.0 (2026-10-19)

the first version. three crates in one workspace, as big-bad-bool and simple_impl are split: `maintenance` is what a crate depends on, `maintenance_macros` the attribute macros it re-exports, `maintenance_core` what the two of them read. the paths below are paths in this repository as of commit 1f136a2: https://github.com/1o-llc/maintenance/tree/1f136a2.

### the attributes

- twelve, each with one job: `#[sensitivity]`, `sensitivity_levels!`, `#[timed_test]`, `#[owner]`, `#[requires_review]`, `#[deprecated_after]`, `#[mechanism]`, `#[boundary]` / `boundary_here!`, `#[supersedes]`, `#[reads_env]`, `#[wire_writer]`, `#[golden]`: crates/maintenance_macros/src/lib.rs.
- every attribute is read by one reader in maintenance_core/src/attrs.rs, the macro while the crate compiles and the check from the source, so the two never disagree about what an attribute says. keys go through simple_impl_attr_kit 0.2.3's grammar and a schema per attribute (`vocabulary`), every key documented; a positional string is read as a literal, as simple_impl's `#[requires("other")]` is. a path key (`levels =`, `writer =`, the path in `#[supersedes]`) is taken written as code or as a string, the code kept as written (`written_tokens`), so the compiler's error inside it lands on it.
- a proc macro sees one item, so what it can check it checks and the rest it marks. each mark is submitted into the binary (`registry::linked()`, inventory) and found again in the source by the checks.
- `sensitivity_levels!` makes a big-bad-bool `#[bitpack]` word, one flag per level, its bit its rank. the shipped levels are the same kind of word, `maintenance_core::levels::Sensitivity`. a level of a word the user declared is checked as a constant's panic carrying `[MAINT1002]`, since the macro cannot see that word.
- `#[timed_test]` decides the threshold while the crate compiles (`max`, then `class`, then maintenance.toml's `default`), names the file with `include_bytes!` so cargo rebuilds when it changes, and runs the body under `timed::Clock`. a test over its threshold fails with `[MAINT2050]` after it returns; every run appends one line to `.maintenance/timings/<run>.jsonl` under a lock file, written back by write-then-rename: crates/maintenance/src/fsio.rs.
- thresholds are written in time_kit's words. time_kit is private to the kit (publish = false), so its 400 ms slot is declared once in maintenance_core/src/duration.rs in the same shape: `500ms`, `2s`, `1m`, `3slots`, `1m30s`.

### the codes

- one `liaise_codes!` table, prefix `MAINT`, 49 codes: maintenance_core/src/codes.rs. the macros raise 0xxx to 5xxx and 8xxx through `SynBuffer`, collecting every problem of an item before the one hard fail; a schema's own message (an unknown key, a wrong kind) follows `MAINT0001` as its context, at the key. nothing in a macro panics.

### the checks

- `maintenance::check` (feature `check`) reads a workspace once with klepto 0.3.0 (`src/` and `tests/`, every reference across crates) and runs `sensitivity`, `mechanism`, `boundary`, `supersedes`, `reads_env`, `golden`, each a `Report` with one serialized shape: crates/maintenance/src/check.rs. a reference klepto cannot resolve is counted and not judged. a path inside a mark (`#[supersedes(crate::old)]`) is not counted as a use of what it names.
- the registry, `.maintenance/registry.json`, is made from the same scan and read by `registry::read`: crates/maintenance/src/registry.rs. format 1, every field always written.
- no command line in the library. the runner (feature `runner`) declares the checks and the timing report as lines of the `maintenance` program with simple_impl_derive 0.2.6's `CompositeShell` and `SimpleSubCommand` over xccute_contract 0.1.2, with named exit statuses (0 clean, 1 findings, 2 refused) and decisions through xccute_runtime 0.1.2's `ExitStatusPolicy`: crates/maintenance/src/runner.rs. the bin (feature `bin`) is the runner's one dispatcher over its arguments, and it refuses any line it cannot read back into a declared line word for word.

### what the derive could not hold

- a line of two fixed words (`maintenance check sensitivity`) would be a `CompositeSubCommand` surface, and a surface's commands do not re-expose a leaf's optional setters or its named exit statuses. so `check` is one leaf whose first word is the check's name, checked by `each` against `runner::CheckName`, and `timed`'s first word is checked the same way. the line reads the same; the gap is simple_impl_derive's to close.

### proof

- `cargo test --workspace --all-features`: 37 tests pass on stable 1.97.1, two of them the trybuild runs, which hold 31 cases (30 refusals, 1 that compiles). on 1.85 the same but the fail matrix, two of whose cases carry the compiler's own wording (E0080, E0432), which changed after 1.85.
- every refusal as the compiler shows it, one file each: crates/maintenance/tests/ui/fail/*.stderr. every attribute on every item it takes, compiling clean: crates/maintenance/tests/ui/pass/every_mark.rs.
- every check on the example workspace, finding for finding: crates/maintenance/tests/example_workspace.rs. the codes a scan raises about the marks themselves, on workspaces the test writes: crates/maintenance/tests/scan_problems.rs. the marks in the binary: crates/maintenance/tests/linked.rs.
- examples/workspace: three crates written so each check has something to find; `cargo test` there passes 7 tests, one of them over its threshold on purpose (`#[should_panic(expected = "MAINT2050")]`).
