use serde::{Deserialize, Serialize};

/// Internationalisation settings for the built-in captcha / auth messages.
///
/// A player's language is resolved as: their Minecraft client locale, then the
/// mirrored server's detected language, then this fallback.
#[derive(Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct I18nConfig {
    /// Language used when a player's language cannot be determined from their
    /// client locale or the mirrored server's MOTD.
    /// One of: english, turkish, german, arabic, dutch, french.
    pub fallback_language: String,

    /// Directory holding the editable per-language translation files
    /// (`<directory>/en.toml`, `tr.toml`, ...). Created with the built-in
    /// defaults on first run. Edit the files and restart to apply changes.
    pub directory: String,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            fallback_language: "english".to_string(),
            directory: "lang".to_string(),
        }
    }
}
