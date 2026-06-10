//! Internationalisation for the captcha / authentication flow and the welcome
//! message.
//!
//! Player-facing strings are loaded from editable per-language files
//! (`lang/<code>.toml`), keyed by language code; see [`messages`]. Built-in
//! languages are written out on first run; dropping a new `<code>.toml` adds a
//! language with no recompilation — just restart. There are no external calls,
//! which keeps the login path fast even under a flood of connections.
//!
//! A player's language is resolved as: the client locale's code, then the
//! mirrored server's detected language, then a configured fallback, then English.
//! Resolution lives in `ServerState` (it needs the loaded files and the mirror).

mod messages;

pub use messages::{I18nError, LanguageMessages, Translations};
#[cfg(test)]
pub use messages::BUILTIN_LANGUAGES;

/// Extracts the language code from a Minecraft client locale such as `"es_es"`,
/// `"fr_ca"` or `"pt-br"` — the part before the region, lowercased.
pub fn locale_code(locale: &str) -> String {
    locale
        .split(['_', '-'])
        .next()
        .unwrap_or(locale)
        .trim()
        .to_ascii_lowercase()
}

/// Resolves a configured `fallback_language` value to a language code. Accepts
/// the full name of a built-in language (e.g. `"spanish"`) or a code directly
/// (e.g. `"es"`), so custom codes also work.
pub fn parse_fallback(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    let code = match lower.as_str() {
        "english" => "en",
        "turkish" | "türkçe" | "turkce" => "tr",
        "german" | "deutsch" => "de",
        "arabic" => "ar",
        "dutch" | "nederlands" => "nl",
        "french" | "francais" | "français" => "fr",
        "spanish" | "español" | "espanol" | "castellano" => "es",
        "italian" | "italiano" => "it",
        // Already a code (built-in or custom): use it as-is.
        other => return other.to_owned(),
    };
    code.to_owned()
}

/// Best-effort detection of a built-in language's code from a server MOTD.
/// Arabic is detected by script; Latin-script languages are scored by distinctive
/// keywords. Returns `None` when there is no clear signal. Custom (file-only)
/// languages are not detected here — they are matched via the client locale.
pub fn detect_from_text(text: &str) -> Option<&'static str> {
    // Arabic script is unambiguous, so short-circuit on it.
    if text.chars().any(is_arabic_char) {
        return Some("ar");
    }

    let lower = text.to_lowercase();

    // Pick the language with the strictly-highest keyword score. A tie (e.g. a
    // generic MOTD like "A Minecraft Server" that only matches the shared
    // "server" token in several languages) is treated as no signal, so the caller
    // falls back rather than guessing arbitrarily by table order.
    let mut best: Option<&'static str> = None;
    let mut best_score = 0;
    let mut tied = false;
    for (code, tokens) in DETECTION_KEYWORDS {
        let score: u32 = tokens
            .iter()
            .filter(|(token, _)| lower.contains(token))
            .map(|(_, weight)| *weight)
            .sum();
        if score > best_score {
            best_score = score;
            best = Some(code);
            tied = false;
        } else if score == best_score && score > 0 {
            tied = true;
        }
    }
    if tied { None } else { best }
}

/// Distinctive MOTD keywords per language code. Distinctive tokens are weighted
/// higher than tokens shared between languages (e.g. "server" appears in English,
/// German and Dutch MOTDs).
const DETECTION_KEYWORDS: &[(&str, &[(&str, u32)])] = &[
    (
        "tr",
        &[
            ("sunucu", 3),
            ("hoş geld", 3),
            ("hoşgeld", 3),
            ("oyuncu", 3),
            ("merhaba", 2),
            ("çevrim", 2),
        ],
    ),
    (
        "de",
        &[
            ("willkommen", 3),
            ("spieler", 3),
            ("kostenlos", 2),
            ("server", 1),
        ],
    ),
    (
        "fr",
        &[
            ("bienvenue", 3),
            ("serveur", 3),
            ("joueur", 3),
            ("rejoign", 2),
        ],
    ),
    ("nl", &[("welkom", 3), ("spelers", 3), ("server", 1)]),
    (
        "es",
        &[
            ("bienvenido", 3),
            ("servidor", 3),
            ("jugadores", 3),
            ("español", 2),
        ],
    ),
    (
        "it",
        &[("benvenuto", 3), ("giocatori", 3), ("italiano", 2)],
    ),
    (
        "en",
        &[("welcome", 3), ("players", 3), ("server", 1), ("join", 1)],
    ),
];

/// Returns `true` for characters that belong to an Arabic Unicode block.
const fn is_arabic_char(c: char) -> bool {
    matches!(c,
        '\u{0600}'..='\u{06FF}' // Arabic
        | '\u{0750}'..='\u{077F}' // Arabic Supplement
        | '\u{08A0}'..='\u{08FF}' // Arabic Extended-A
        | '\u{FB50}'..='\u{FDFF}' // Arabic Presentation Forms-A
        | '\u{FE70}'..='\u{FEFF}' // Arabic Presentation Forms-B
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_code_extracts_prefix() {
        assert_eq!(locale_code("tr_tr"), "tr");
        assert_eq!(locale_code("en_GB"), "en");
        assert_eq!(locale_code("fr_ca"), "fr");
        assert_eq!(locale_code("pt-br"), "pt");
        assert_eq!(locale_code("ja_jp"), "ja");
    }

    #[test]
    fn parse_fallback_accepts_names_and_codes() {
        assert_eq!(parse_fallback("spanish"), "es");
        assert_eq!(parse_fallback("Italiano"), "it");
        assert_eq!(parse_fallback("english"), "en");
        assert_eq!(parse_fallback("es"), "es");
        // Unknown name is treated as a (custom) code.
        assert_eq!(parse_fallback("pt"), "pt");
    }

    #[test]
    fn detect_arabic_by_script() {
        assert_eq!(detect_from_text("مرحبا بكم في الخادم"), Some("ar"));
    }

    #[test]
    fn detect_latin_by_keywords() {
        assert_eq!(detect_from_text("Hoş geldin! Sunucumuza katıl"), Some("tr"));
        assert_eq!(detect_from_text("Willkommen auf dem Server"), Some("de"));
        assert_eq!(detect_from_text("Bienvenue sur le serveur"), Some("fr"));
        assert_eq!(detect_from_text("Welkom op de server, spelers!"), Some("nl"));
        assert_eq!(detect_from_text("¡Bienvenido al servidor!"), Some("es"));
        assert_eq!(detect_from_text("Benvenuto, giocatori!"), Some("it"));
        assert_eq!(detect_from_text("Welcome players, join now"), Some("en"));
        assert_eq!(detect_from_text("?????"), None);
    }

    #[test]
    fn generic_motd_is_ambiguous_not_guessed() {
        // The shared "server" token alone must not pick a language.
        assert_eq!(detect_from_text("A Minecraft Server"), None);
        assert_eq!(detect_from_text("The server is online"), None);
        // A distinctive keyword still wins even alongside the shared token.
        assert_eq!(detect_from_text("Willkommen auf dem Server"), Some("de"));
    }
}
