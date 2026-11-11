//! Cheat packs: the v1 schema, validator, hotkey parsing, the pack store and
//! the trainer runtime that applies a pack to an attached game.
//!
//! Packs are declarative data. The only ops are `value` (read/write/freeze a
//! typed value at a resolved address) and `patch` (replace a few checked bytes
//! at an AOB-located site with instructions from a safe allowlist). Everything
//! goes through `jb-core`, so the anti-cheat guard applies.

pub mod community;
pub mod ct;
pub(crate) mod ct_script;
pub mod heal;
pub mod hotkey;
pub mod numbers;
pub mod pad;
pub mod schema;
pub mod settings;
pub mod share;
pub mod store;
pub mod trainer;
pub mod validate;
pub mod web;

pub use hotkey::Hotkey;
pub use pad::PadChord;
pub use schema::{Cheat, Impl, Pack, Ui};
pub use settings::{GameSettings, Preset, PresetCheat};
pub use trainer::{BuildMatch, CheatState, Trainer};
pub use validate::{Issue, LoadError, Severity, parse, validate};

/// The JSON Schema for pack v1, for editors and the AI creator.
pub const SCHEMA_JSON: &str = include_str!("../schema/pack.v1.json");

#[cfg(test)]
mod tests {
    #[test]
    fn json_schema_is_valid_json_and_lists_every_kind() {
        let schema: serde_json::Value = serde_json::from_str(super::SCHEMA_JSON).unwrap();
        assert_eq!(schema["$id"], "https://opentrainer.org/schema/pack.v1.json");
        let text = super::SCHEMA_JSON;
        for word in [
            "\"value\"",
            "\"patch\"",
            "\"rip32\"",
            "\"abs32\"",
            "\"saveCodes\"",
            "\"hold\"",
        ] {
            assert!(text.contains(word), "schema is missing {word}");
        }
    }
}
