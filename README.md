# maintenance

Attributes that say what an item is, and checks that hold a workspace to it.

A proc macro sees one item at a time. So each attribute here does two things: it checks what it can while the crate
compiles (every refusal is a `[MAINT....]` code at the token it is about), and it leaves a mark that something
reading the whole workspace can find. The checks read the whole workspace through klepto: every item, every
reference across crates, and every mark, read by the same readers the macros use. What the marks say is written
into one file, `.maintenance/registry.json`, for other tools to read.

Three crates, as big-bad-bool and simple_impl are split:

| crate | what it is |
|---|---|
| `maintenance` | what you depend on: the attributes (re-exported), the registry, the timed-test clock and report, the checks (feature `check`), the declared runner (feature `runner`) and its bin (feature `bin`) |
| `maintenance_macros` | the attribute macros; their expansions name `::maintenance` |
| `maintenance_core` | what both read: the code table, the sensitivity levels, thresholds, `maintenance.toml`, every attribute's reader |

```toml
[dependencies]
maintenance = "0.1"
```

Nothing is published yet; the name `maintenance` is held on crates.io at 0.0.1. CI (.github/workflows/ci.yml) publishes
the three crates from main, in dependency order, only once CHANGELOG.md's entry for the version has its date.

## The attributes

| attribute | its one job |
|---|---|
| `#[sensitivity(secret)]` | how sensitive an item is; the check reports a less sensitive item reaching into a more sensitive one |
| `sensitivity_levels! { pub Word { public = 0, ... } }` | a workspace's own level word (the shipped one is `public`, `internal`, `confidential`, `secret`, `key_material`) |
| `#[timed_test(max = "2s")]` | fail a test that takes longer than its threshold, and report every test's time |
| `#[owner("platform")]` | who keeps an item |
| `#[requires_review(reason = "...")]` | a change to the item needs a second reader |
| `#[deprecated_after("2026-12-01", reason = "...")]` | `#[deprecated]` until the date, a compile error after it |
| `#[mechanism("retry policy")]` | this item is the one place for a rule; a second claim is refused by the check |
| `#[boundary(allowed = ["api"])]`, `boundary_here!(allowed = ["api"]);` | which crates may use what is inside a module or a crate |
| `#[supersedes("crate::old")]` | this item replaces `old`; the check lists what still uses `old` |
| `#[reads_env("KEY")]` | this item reads an environment key; the check refuses a key no declaration file declares |
| `#[wire_writer]` | this function writes bytes something else reads; the check refuses one with no golden test |
| `#[golden(writer = path::to::writer)]` | this test pins the writer's bytes |

`#[sensitivity]`, `#[owner]`, `#[requires_review]`, `#[mechanism]`, `#[supersedes]`, `#[reads_env]`, `#[golden]` and
`#[wire_writer]` go on a fn, struct, enum, union, trait, type alias, const, static, inline `mod` or `impl`.
`#[deprecated_after]` goes on all of those but an `impl`. `#[boundary]` goes on an inline `mod`; a crate root or a
module in its own file takes `boundary_here!` inside it, since an attribute macro cannot go there on stable Rust.
A mark on a fn is submitted from inside its body, so a mark on a method stays inside its impl.

### `#[sensitivity(level)]`

```rust
use maintenance::sensitivity;

#[sensitivity(secret)]
pub fn open_vault() -> Vault { /* ... */ }

#[sensitivity(key_material)]
impl MasterKey { /* every method here is key_material */ }
```

The level is one bare name. The shipped levels are the flags of `maintenance::Sensitivity`, a big-bad-bool word:
`public` at bit 0, `internal` 1, `confidential` 2, `secret` 3, `key_material` 4. A level's bit is its rank. A name
the word does not have is refused while the crate compiles (`MAINT1002`).

An item's level is its own, else its impl block's, else (for a method) its type's, else the nearest enclosing
module's. An item nothing marks is `[sensitivity] unmarked` in maintenance.toml, the lowest level when that is absent.

### `sensitivity_levels!`

```rust
maintenance::sensitivity_levels! {
    pub VaultLevels { public = 0, internal = 1, confidential = 2, secret = 3, key_material = 4, root_of_trust = 5 }
}

#[maintenance::sensitivity(root_of_trust, levels = crate::VaultLevels)]
pub fn root_seed() -> u64 { 42 }
```

A `#[bitpack]` word with one flag per level, so the crate declaring one depends on `big-bad-bool` too. `levels =`
is a path written as code (or as a string); the compiler checks it, and a level the word does not have fails the
build as a constant's panic carrying `[MAINT1002]`, since the macro cannot see a word the user declared. The
workspace has one level table: its one `sensitivity_levels!`, or the shipped levels when there is none. The
declaration is read by `maintenance_core::levels::parse_declaration`, the macro and the check alike.

### `#[timed_test]`

```rust
use maintenance::timed_test;

#[timed_test(max = "2s")]
fn loads_the_catalog() { /* ... */ }

#[timed_test(class = "integration")]
fn round_trip() { /* ... */ }

#[timed_test]                      // the default class from maintenance.toml
fn quick() -> Result<(), String> { Ok(()) }

#[timed_test(max = "500ms")]       // above the runtime's own test attribute
#[tokio::test]
async fn async_status() { /* ... */ }
```

It makes the fn a `#[test]` (or keeps the runtime's `#[tokio::test]`, which it must sit above), runs the body under
a clock, and fails the test with `[MAINT2050] the test ran past its threshold: m::t took 2s310ms; its threshold is
2s (max)` when it returns late. `#[should_panic]`, `#[ignore]` and a `Result` return keep working. The threshold is
decided while the crate compiles: `max`, else `class`, else the file's `default` class; a class the file and the
shipped classes do not have is refused (`MAINT2003`), and so is a test with no threshold at all (`MAINT2004`).

Thresholds are written in time_kit's words. time_kit counts slots of 400 ms (`SLOT_MS`) and is private to the kit,
so the slot is declared once in `maintenance_core::duration` in the same shape. A threshold is whole numbers with
units, added: `500ms`, `2s`, `1m`, `3slots`, `1m30s`.

Every run of every timed test appends one JSON line to `<root>/.maintenance/timings/<run>.jsonl` (under a lock, the
file written back whole by write-then-rename, so parallel tests and parallel test binaries each land their line).
`<root>` is the folder of the `maintenance.toml` the threshold came from, else the crate's folder; `MAINTENANCE_RUN`
names the run (`latest` when unset) and `MAINTENANCE_TIMINGS` another folder. A test is failed after it returns; one
that never returns is not stopped.

### `#[owner]`, `#[requires_review]`

```rust
#[maintenance::owner("platform")]
#[maintenance::requires_review(reason = "opens the key store")]
pub fn open_vault() -> Vault { /* ... */ }
```

An owner is inherited the way a level is: from the impl, the type, the enclosing module.

### `#[deprecated_after]`

```rust
#[maintenance::deprecated_after("2026-12-01", reason = "use open_vault")]
pub fn legacy_open() -> Vault { open_vault() }
```

Until the date the item is `#[deprecated(note = "deprecated after 2026-12-01: use open_vault")]`; after it, the
build fails at the date (`MAINT3002`). The date is compared with the machine's clock (UTC) when the crate compiles,
so a crate that does not rebuild does not see the day pass until it does: a clean build in CI does.

### `#[mechanism]`

```rust
#[maintenance::mechanism("retry policy")]
pub fn backoff(attempt: u32) -> u64 { 100 * u64::from(attempt) }
```

One rule in one place: a second item anywhere in the workspace claiming `"retry policy"` is `MAINT6010`.

### `#[boundary]`, `boundary_here!`

```rust
// crates/vault/src/lib.rs: only the api crate may use the vault
maintenance::boundary_here!(allowed = ["api"]);

#[maintenance::boundary(allowed = [])]
pub mod sealed { pub fn inner() -> u8 { 1 } }
```

The crate a boundary is written in may always use what is inside. Any other crate not listed that references or
imports an item inside is `MAINT6020`; for a crate root, so is a Cargo dependency on the crate.

### `#[supersedes]`

```rust
#[maintenance::supersedes("crate::legacy_open")]
pub fn open_vault() -> Vault { /* ... */ }
```

The path is written as a string or as code, `crate::`, `self::` and `super::` read from where it is written. The
check lists every item that still references or imports the old one (`MAINT6030`), says so when nothing does and
the old one can go (`MAINT6031`), and says so when it is already gone (`MAINT6032`). These are notes, not failures.

### `#[reads_env]`

```rust
#[maintenance::reads_env("VAULT_PATH")]
pub fn vault_path() -> String { std::env::var("VAULT_PATH").unwrap_or_else(|_| "/var/vault".into()) }
```

The check reads every file a glob matches and the keys at a dotted path inside each (a table's keys, or a list of
strings, or a list of tables' `name`), and refuses a key read nowhere declared (`MAINT6040`).

### `#[wire_writer]`, `#[golden]`

```rust
#[maintenance::wire_writer]
pub fn encode_record(id: u32, body: &[u8]) -> Vec<u8> { /* ... */ }

// crates/api/tests/golden.rs
#[maintenance::golden(writer = vault::encode_record)]
#[test]
fn record_bytes_are_pinned() {
    assert_eq!(vault::encode_record(1, b"ok"), [0, 0, 0, 1, b'o', b'k']);
}
```

`writer` is a path written as code (or as a string); the macro names it in the test, so a writer that does not
exist is the compiler's error, on the path. The check refuses a `#[wire_writer]` no `#[golden]` names
(`MAINT6050`) and a `#[golden]` naming a function that is not a `#[wire_writer]` (`MAINT6051`).

## maintenance.toml

The nearest one at or above a crate's folder, for the thresholds; the one beside the workspace's `Cargo.toml`, for
the checks.

```toml
[timed_test]
default = "unit"          # the class a bare #[timed_test] gets
[timed_test.classes]      # added to the shipped classes: unit 500ms, integration 10s, slow 1m
unit = "200ms"
integration = "2s"

[sensitivity]
unmarked = "public"       # the level of an item nothing marks; the lowest level when absent
max_rise = 0              # how many levels above its own an item may reach; 0 when absent
[[sensitivity.allow]]     # a reach looked at and accepted: reported as a note
from = "api::login"
to = "vault::check_password"

[[env.declared_in]]       # where environment keys are declared, for #[reads_env]
glob = "deploy/*.toml"    # relative to the workspace root
path = "env"              # the dotted path inside each file
```

Every table refuses a key it does not know.

## The checks

`maintenance::check` (feature `check`). There is no command line in the library: a tool calls these, or runs the
declared lines below.

```rust
use maintenance::check::{self, Workspace};

let ws = Workspace::scan("examples/workspace".as_ref())?;   // klepto, once
let reports = ws.all();                                     // or ws.sensitivity(), ws.mechanism(), ...
ws.write_registry()?;                                       // .maintenance/registry.json

// or one check, scanning for itself
let report = check::reads_env("examples/workspace".as_ref(), "deploy/*.toml", "env")?;
```

`check::sensitivity`, `check::mechanism`, `check::boundary`, `check::supersedes`, `check::reads_env(root, glob,
key_path)`, `check::golden`, `check::all`, `check::registry`, `check::write_registry`. Each returns a `Report`:

```json
{
  "format": 1,
  "check": "sensitivity",
  "workspace": "examples/workspace",
  "passed": false,
  "findings": [
    { "code": "MAINT6001", "message": "[MAINT6001] a lower sensitivity level reaches into a higher one: api::status (public) uses vault::open_vault (secret)",
      "item": "api::status", "target": "vault::open_vault", "file": "crates/api/src/lib.rs", "line": 14 }
  ],
  "notes": [],
  "counts": { "items_with_a_level": 13, "references_judged": 20, "unresolved_references": 14 }
}
```

`findings` fail the check; `notes` do not. `file` is relative to the workspace root. Findings and notes are sorted
by place, so a report reads the same whatever order klepto read the files in.

The sensitivity check judges every reference from a fn or method to an item with a level: calls, and the types and
items its body and signature name. A reference klepto cannot resolve (a method call on a receiver it cannot type) is
not judged and not guessed; `unresolved_references` counts them.

## The runner

Feature `runner`: the checks and the timing report declared as lines of the `maintenance` program, by
simple_impl_derive's `CompositeShell` and `SimpleSubCommand` over xccute_contract 0.1.2, so a tool such as custodian
routes to them as commands it can preview and run. Feature `bin` adds the `maintenance` program that runs them.

```text
maintenance check <which> --workspace <root> [--registry <file>] [--decl <glob> --key <path>]
maintenance timed report <dir>
```

```rust
use maintenance::runner::{CheckName, Maintenance};
use xccute_contract::CompositeShellCommand;

let line = Maintenance::check_of(CheckName::ReadsEnv, "examples/workspace").decl("deploy/*.toml").key("env");
assert_eq!(line.build_display(), "maintenance check reads_env --workspace examples/workspace --decl deploy/*.toml --key env");
let outcome = maintenance::runner::execute(&line);        // in this process, as the bin runs it
assert_eq!((outcome.code, outcome.word), (1, "findings"));
```

`<which>` is `sensitivity`, `mechanism`, `boundary`, `supersedes`, `reads_env`, `golden` or `all`. Each line names
its exit statuses: 0 `clean`, 1 `findings`, 2 `refused`. On stdout is one JSON document: for `check`, `{ "format": 1,
"workspace": ..., "reports": [Report, ...], "registry": null | "<file>" }`; for `timed report`, the timing report
below; for status 2, `{ "format": 1, "code": "MAINT9001", "message": "..." }`. The bin reads its words back into
the declared line and refuses any line that is not one of these, word for word. The library is the one
implementation; the runner is its facade.

```text
$ cargo run -p maintenance --features bin -- check all --workspace examples/workspace --registry /tmp/registry.json
$ cargo run -p maintenance --features bin -- timed report examples/workspace
```

## The registry

`<workspace>/.maintenance/registry.json`, written by `check::write_registry` (or `--registry <file>`) and read by
`maintenance::registry::read(workspace_root)`. Format 1:

```json
{
  "format": 1,
  "levels": ["public", "internal", "confidential", "secret", "key_material", "root_of_trust"],
  "items": [
    {
      "id": "vault::open_vault",
      "package": "vault",
      "kind": "fn",
      "file": "crates/vault/src/lib.rs",
      "line": 39,
      "sensitivity": "secret",
      "sensitivity_from": "vault::open_vault",
      "owner": null,
      "owner_from": null,
      "review": { "reason": "opens the key store" },
      "deprecated_after": null,
      "mechanism": "vault opening",
      "boundary": null,
      "supersedes": "vault::legacy_open",
      "reads_env": [],
      "golden_writer": null,
      "wire_writer": false
    }
  ]
}
```

`levels` is the level table, lowest first: a level's place in it is its rank. `id` is klepto's canonical path;
`line` is where the item starts (its first attribute or doc line). `sensitivity` and `owner` are the item's own or
inherited, and `*_from` names the item they are written on; every other field is the item's own. Every marked item is
in it, and every item that takes a level or an owner from its impl, type or module. Every field is always written.
`Registry::rank`, `Registry::get` and `Registry::at_or_above("secret")` answer from it. Inside a running binary the
same marks are `maintenance::registry::linked()`.

## The timing report

`maintenance::timed::report(dir)` (or `maintenance timed report <dir>`): the latest run of every test, slowest first.

```json
{ "format": 1, "source": "examples/workspace", "tests": [Record, ...], "over": 1, "unreadable": 0 }
```

A `Record` is one line of the run file: `test`, `file`, `line`, `ms`, `max_ms`, `class`, `threshold_from` (`max`,
`class` or `default`), `outcome` (`passed`, `over`, `panicked`), `at_unix_ms`. `TimingReport::render(top)` is the
same as text.

## The codes

One `liaise_codes!` table, `maintenance_core::codes`. The macros raise 0xxx to 5xxx and 8xxx while a crate compiles,
a timed test 2050 and 2051 while it runs, the checks 6xxx, the registry 7xxx, the runner 9xxx.

| code | name | message |
|---|---|---|
| `MAINT0001` | `AttrUnreadable` | the attribute does not read |
| `MAINT0002` | `WrongItem` | this attribute does not go on this kind of item |
| `MAINT0003` | `Repeated` | the attribute is written twice on one item |
| `MAINT1001` | `LevelMissing` | #[sensitivity(level)] names one level: #[sensitivity(secret)] |
| `MAINT1002` | `UnknownLevel` | unknown sensitivity level |
| `MAINT1003` | `TwoLevels` | #[sensitivity] names one level; an item has one |
| `MAINT1010` | `LevelsEmpty` | sensitivity_levels! declares at least one level |
| `MAINT1011` | `LevelBitTaken` | two levels are declared at one bit |
| `MAINT1012` | `LevelNameTaken` | a level is declared twice |
| `MAINT1013` | `LevelBitTooHigh` | a level's bit is its rank in a u32 word: 0 to 31 |
| `MAINT1014` | `LevelsUnreadable` | sensitivity_levels! reads `pub Name { level = bit, ... }` |
| `MAINT2001` | `NotATestFn` | #[timed_test] goes on a test fn: no arguments, no generics |
| `MAINT2002` | `BadDuration` | a threshold is a whole number with a unit: 500ms, 2s, 1m, 3slots, or several: 1m30s |
| `MAINT2003` | `UnknownClass` | unknown test class |
| `MAINT2004` | `NoThreshold` | no threshold: write max = "2s" or class = "unit", or a default class in maintenance.toml |
| `MAINT2005` | `ThresholdFile` | maintenance.toml does not read |
| `MAINT2006` | `AsyncNeedsRuntime` | an async test needs its runtime's test attribute below #[timed_test]: #[tokio::test] |
| `MAINT2050` | `OverThreshold` | the test ran past its threshold |
| `MAINT2051` | `ReportUnwritten` | the timing report could not be written |
| `MAINT3001` | `BadDate` | a date is written YYYY-MM-DD |
| `MAINT3002` | `DatePassed` | this item's deprecation date has passed; remove it, or move the date with a reason |
| `MAINT3003` | `ReasonMissing` | #[deprecated_after] says why: #[deprecated_after("2026-12-01", reason = "...")] |
| `MAINT4001` | `OwnerMissing` | #[owner] names its owner as a string: #[owner("platform")] |
| `MAINT5001` | `ReviewReasonMissing` | #[requires_review] says why: #[requires_review(reason = "...")] |
| `MAINT8001` | `MechanismName` | #[mechanism] names its rule as a string: #[mechanism("retry policy")] |
| `MAINT8011` | `BoundaryAllowed` | a boundary lists the crates that may use it: #[boundary(allowed = ["api"])] |
| `MAINT8021` | `SupersedesPath` | #[supersedes] names the item it replaces as a path: #[supersedes("crate::old::thing")] |
| `MAINT8031` | `ReadsEnvKey` | #[reads_env] names the keys read here as strings: #[reads_env("DATABASE_URL")] |
| `MAINT8041` | `GoldenWriter` | #[golden] names the writer it pins: #[golden(writer = path::to::writer)] |
| `MAINT8051` | `WireWriterArgs` | #[wire_writer] takes nothing: #[wire_writer] |
| `MAINT6001` | `CallRises` | a lower sensitivity level reaches into a higher one |
| `MAINT6002` | `LevelNotInTable` | an item names a level the workspace's level table does not have |
| `MAINT6003` | `TwoLevelTables` | the workspace declares two different level tables; it has one |
| `MAINT6004` | `CheckConfig` | maintenance.toml does not read |
| `MAINT6005` | `WorkspaceUnread` | the workspace could not be read |
| `MAINT6006` | `MarkUnread` | a mark in the source does not read |
| `MAINT6010` | `MechanismTwice` | two items claim one mechanism; a rule has one place |
| `MAINT6020` | `BoundaryCrossed` | a crate the boundary does not allow uses what is inside it |
| `MAINT6030` | `SupersededStillUsed` | the item this one supersedes still has users |
| `MAINT6031` | `SupersededFree` | the item this one supersedes has no users left; it can go |
| `MAINT6032` | `SupersededGone` | the item this one supersedes is not in the workspace |
| `MAINT6040` | `EnvUndeclared` | an environment key is read where no declaration file declares it |
| `MAINT6050` | `WriterWithoutGolden` | a wire writer has no golden test pinning its bytes |
| `MAINT6051` | `GoldenWriterUnknown` | a golden test names a writer that is not a #[wire_writer] |
| `MAINT7001` | `RegistryUnreadable` | the registry file could not be read |
| `MAINT7002` | `RegistryFormat` | the registry file is not this format |
| `MAINT7003` | `RegistryUnwritten` | the registry file could not be written |
| `MAINT9001` | `LineRefused` | the line is not a declared maintenance line |
| `MAINT9002` | `RunRefused` | the line could not run |

An attribute's own grammar (an unknown key, a key of the wrong kind) is simple_impl_attr_kit's message after
`MAINT0001`, at the key: ``[MAINT0001] the attribute does not read: unknown attribute key `maks`; did you mean `max`? known keys: class, max``.

## The example workspace

`examples/workspace` is three crates (vault, api, cli) with its own level word, marks across the crate line, timed
tests, a golden test and a declaration file, written so each check has something to find.

```text
$ cd examples/workspace && cargo test          # the timed tests write .maintenance/timings/latest.jsonl
$ cd ../.. && cargo run -p maintenance --features check --example check -- examples/workspace
```

It prints every report, writes the registry, and prints the timing report (exit 1: the workspace has findings):

```text
scan check, examples/workspace: passed, 0 findings, 0 notes
  counts: files 5, items 38, marked_items 23
sensitivity check, examples/workspace: failed, 9 findings, 1 notes
  [MAINT6001] a lower sensitivity level reaches into a higher one: api::health (public, unmarked) uses vault::MasterKey::fingerprint (key_material, from vault::MasterKey) (crates/api/src/lib.rs:7)
  [MAINT6001] a lower sensitivity level reaches into a higher one: api::health (public, unmarked) uses vault::MasterKey::load (key_material) (crates/api/src/lib.rs:7)
  [MAINT6001] a lower sensitivity level reaches into a higher one: api::status (public) uses vault::open_vault (secret) (crates/api/src/lib.rs:14)
  [MAINT6001] a lower sensitivity level reaches into a higher one: api::rotate (internal) uses vault::root_seed (root_of_trust) (crates/api/src/lib.rs:33)
  [MAINT6001] a lower sensitivity level reaches into a higher one: api::old_path (public, unmarked) uses vault::Vault::unseal (secret, from vault::<impl Vault>) (crates/api/src/lib.rs:39)
  [MAINT6001] a lower sensitivity level reaches into a higher one: api::old_path (public, unmarked) uses vault::legacy_open (secret) (crates/api/src/lib.rs:39)
  [MAINT6001] a lower sensitivity level reaches into a higher one: cli::main (public, unmarked) uses vault::audit_log (internal) (crates/cli/src/main.rs:4)
  [MAINT6001] a lower sensitivity level reaches into a higher one: vault::open_vault (secret) uses vault::MasterKey::load (key_material) (crates/vault/src/lib.rs:45)
  [MAINT6001] a lower sensitivity level reaches into a higher one: vault::Vault::unseal (secret) uses vault::MasterKey::fingerprint (key_material, from vault::MasterKey) (crates/vault/src/lib.rs:60)
  note [MAINT6001] a lower sensitivity level reaches into a higher one: api::login (public) uses vault::check_password (secret) (allowed by maintenance.toml) (crates/api/src/lib.rs:21)
  counts: items_with_a_level 13, references_judged 20, unresolved_references 14
mechanism check, examples/workspace: failed, 1 findings, 0 notes
  [MAINT6010] two items claim one mechanism; a rule has one place: "retry policy" is api::retry_delay (crates/api/src/lib.rs:42) and vault::backoff (crates/vault/src/lib.rs:106)
  counts: mechanisms 2
boundary check, examples/workspace: failed, 3 findings, 0 notes
  [MAINT6020] a crate the boundary does not allow uses what is inside it: crate cli depends on crate vault; vault allows api
  [MAINT6020] a crate the boundary does not allow uses what is inside it: api::peek (crate api) uses vault::sealed::inner; vault::sealed allows none (crates/api/src/lib.rs:50)
  [MAINT6020] a crate the boundary does not allow uses what is inside it: cli::main (crate cli) uses vault::audit_log; vault allows api (crates/cli/src/main.rs:4)
  counts: boundaries 2
supersedes check, examples/workspace: passed, 0 findings, 2 notes
  note [MAINT6030] the item this one supersedes still has users: vault::open_vault supersedes vault::legacy_open, still used by 1: api::old_path (crates/api/src/lib.rs:39) (crates/vault/src/lib.rs:82)
  note [MAINT6031] the item this one supersedes has no users left; it can go: vault::seal_v2 supersedes vault::seal_v1 (crates/vault/src/lib.rs:138)
  counts: remaining_users 1
reads_env check, examples/workspace: failed, 1 findings, 0 notes
  [MAINT6040] an environment key is read where no declaration file declares it: VAULT_DEBUG is read by vault::debug_unseal; declared in: deploy/*.toml at env (crates/vault/src/lib.rs:118)
  counts: declaration_files 1, keys_declared 1, keys_read 2
golden check, examples/workspace: failed, 1 findings, 0 notes
  [MAINT6050] a wire writer has no golden test pinning its bytes: vault::encode_header (crates/vault/src/lib.rs:132)
  counts: goldens 1, wire_writers 2
registry: examples/workspace/.maintenance/registry.json (27 items; 10 at secret or above)
slowest tests, the latest run of each: 6 shown of 6
      took         threshold   of it  outcome   test
      43ms    2s integration      2%  passed    timing::login_round_trip  (crates/api/tests/timing.rs:7)
      22ms         1ms tight   2200%  OVER      timing::over_its_threshold  (crates/api/tests/timing.rs:20)
      20ms             500ms      4%  passed    timing::async_status  (crates/api/tests/timing.rs:26)
       0ms        200ms unit      0%  passed    api::tests::health_is_quick  (crates/api/src/lib.rs:58)
       0ms                1s      0%  passed    api::tests::status_reads_the_vault  (crates/api/src/lib.rs:63)
       0ms        200ms unit      0%  passed    timing::admin_report_counts  (crates/api/tests/timing.rs:13)
1 over threshold [MAINT2050]
```

(`timing::over_its_threshold` is over on purpose: it is `#[should_panic(expected = "MAINT2050")]`, so the clock's
failure is what makes it pass.) `crates/maintenance/tests/example_workspace.rs` holds the same findings as a test.

## Tests

`cargo test --workspace --all-features`: the readers, the thresholds, the dates, the code table, the registry file,
the timing report and its lock, the runner's lines, every check on the example workspace and on workspaces a test
writes, and `tests/ui`: one trybuild case per refusal (`tests/ui/fail`, the compiler's output beside each) and one
case with every attribute on every kind of item it takes (`tests/ui/pass`). On Rust 1.85 the fail matrix is skipped:
two of its cases carry the compiler's own wording (E0080, E0432), which changed after 1.85; the codes are the same.

## Ownership

Developed and published by James Hunt. The repository is held by the 1o organization
(https://github.com/1o-llc). Assignment of the copyright to 1o is pending documentation. The license texts
(LICENSE-MIT, LICENSE-APACHE) are left as they were found.

## License

MIT OR Apache-2.0.
