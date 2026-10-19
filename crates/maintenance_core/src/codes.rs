//! Every code this set raises, in one liaise table. Rendered `[MAINT1002] message`.
//!
//! The macros raise 0xxx to 5xxx and 8xxx while a crate compiles (through `SynBuffer`, or as a constant's panic for
//! a level word the macro cannot see); the runtime raises 2050 when a timed test runs over and 2051 when its line cannot be written, the checker 6xxx, and
//! reading or writing the registry 7xxx. One table, so a code means one thing wherever it is printed.

pub use liaise_proc_macro::LiaiseCode;

liaise_proc_macro::liaise_codes! {
    prefix: "MAINT",
    pub Code {
        // 0xxx: any of the attributes
        AttrUnreadable = 1 => "the attribute does not read",
        WrongItem = 2 => "this attribute does not go on this kind of item",
        Repeated = 3 => "the attribute is written twice on one item",

        // 1xxx: #[sensitivity] and sensitivity_levels!
        LevelMissing = 1001 => "#[sensitivity(level)] names one level: #[sensitivity(secret)]",
        UnknownLevel = 1002 => "unknown sensitivity level",
        TwoLevels = 1003 => "#[sensitivity] names one level; an item has one",
        LevelsEmpty = 1010 => "sensitivity_levels! declares at least one level",
        LevelBitTaken = 1011 => "two levels are declared at one bit",
        LevelNameTaken = 1012 => "a level is declared twice",
        LevelBitTooHigh = 1013 => "a level's bit is its rank in a u32 word: 0 to 31",
        LevelsUnreadable = 1014 => "sensitivity_levels! reads `pub Name { level = bit, ... }`",

        // 2xxx: #[timed_test]
        NotATestFn = 2001 => "#[timed_test] goes on a test fn: no arguments, no generics",
        BadDuration = 2002 => "a threshold is a whole number with a unit: 500ms, 2s, 1m, 3slots, or several: 1m30s",
        UnknownClass = 2003 => "unknown test class",
        NoThreshold = 2004 => "no threshold: write max = \"2s\" or class = \"unit\", or a default class in maintenance.toml",
        ThresholdFile = 2005 => "maintenance.toml does not read",
        AsyncNeedsRuntime = 2006 => "an async test needs its runtime's test attribute below #[timed_test]: #[tokio::test]",
        OverThreshold = 2050 => "the test ran past its threshold",
        ReportUnwritten = 2051 => "the timing report could not be written",

        // 3xxx: #[deprecated_after]
        BadDate = 3001 => "a date is written YYYY-MM-DD",
        DatePassed = 3002 => "this item's deprecation date has passed; remove it, or move the date with a reason",
        ReasonMissing = 3003 => "#[deprecated_after] says why: #[deprecated_after(\"2026-12-01\", reason = \"...\")]",

        // 4xxx: #[owner]
        OwnerMissing = 4001 => "#[owner] names its owner as a string: #[owner(\"platform\")]",

        // 5xxx: #[requires_review]
        ReviewReasonMissing = 5001 => "#[requires_review] says why: #[requires_review(reason = \"...\")]",

        // 8xxx: #[mechanism], #[boundary] / boundary!, #[supersedes], #[reads_env], #[golden], #[wire_writer]
        MechanismName = 8001 => "#[mechanism] names its rule as a string: #[mechanism(\"retry policy\")]",
        BoundaryAllowed = 8011 => "a boundary lists the crates that may use it: #[boundary(allowed = [\"api\"])]",
        SupersedesPath = 8021 => "#[supersedes] names the item it replaces as a path: #[supersedes(\"crate::old::thing\")]",
        ReadsEnvKey = 8031 => "#[reads_env] names the keys read here as strings: #[reads_env(\"DATABASE_URL\")]",
        GoldenWriter = 8041 => "#[golden] names the writer it pins: #[golden(writer = path::to::writer)]",
        WireWriterArgs = 8051 => "#[wire_writer] takes nothing: #[wire_writer]",

        // 6xxx: the checker (maintenance check)
        CallRises = 6001 => "a call from a lower sensitivity level into a higher one",
        LevelNotInTable = 6002 => "an item names a level the workspace's level table does not have",
        TwoLevelTables = 6003 => "the workspace declares two different level tables; it has one",
        CheckConfig = 6004 => "maintenance.toml does not read",
        WorkspaceUnread = 6005 => "the workspace could not be read",
        MarkUnread = 6006 => "a mark in the source does not read",
        MechanismTwice = 6010 => "two items claim one mechanism; a rule has one place",
        BoundaryCrossed = 6020 => "a crate the boundary does not allow uses what is inside it",
        SupersededStillUsed = 6030 => "the item this one supersedes still has users",
        SupersededFree = 6031 => "the item this one supersedes has no users left; it can go",
        SupersededGone = 6032 => "the item this one supersedes is not in the workspace",
        EnvUndeclared = 6040 => "an environment key is read where no declaration file declares it",
        WriterWithoutGolden = 6050 => "a wire writer has no golden test pinning its bytes",
        GoldenWriterUnknown = 6051 => "a golden test names a writer that is not a #[wire_writer]",

        // 7xxx: the registry file
        RegistryUnreadable = 7001 => "the registry file could not be read",
        RegistryFormat = 7002 => "the registry file is not this format",
        RegistryUnwritten = 7003 => "the registry file could not be written",
    }
}

/// `[MAINT1002] unknown sensitivity level: ctx`, the text a refusal prints.
pub fn render_ctx(code: Code, ctx: impl core::fmt::Display) -> String {
    format!("{}: {ctx}", code.render())
}
