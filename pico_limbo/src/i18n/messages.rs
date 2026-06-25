use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};
use thiserror::Error;

/// Language codes that ship with built-in translations. Their files are written
/// to the `lang/` directory on first run. Additional languages can be added by
/// dropping a `<code>.toml` file in that directory — no recompilation needed.
pub const BUILTIN_LANGUAGES: &[&str] = &["en", "tr", "de", "ar", "nl", "fr", "es", "it"];

#[derive(Debug, Error)]
pub enum I18nError {
    #[error("failed to read/write translation files: {0}")]
    Io(#[from] io::Error),
    #[error("failed to parse a translation file: {0}")]
    Deserialize(#[from] toml::de::Error),
    #[error("failed to serialize default translations: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// All translatable, server-generated messages for a single language.
///
/// Loaded from `lang/<code>.toml`. Any field left empty falls back to the
/// built-in default for that language (English for a custom language), so a file
/// can never be "half broken".
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct LanguageMessages {
    /// Join welcome message (`MiniMessage` formatting allowed), shown as a
    /// full-screen title. Defined per language in this file; if left empty, the
    /// localized built-in default for the language is used.
    pub welcome: String,
    /// Label before a "type this number" challenge.
    pub captcha_copy_label: String,
    /// Label before an arithmetic challenge (`8 - 3`, `2 × 4`, ...).
    pub captcha_solve_label: String,
    /// Label before a "type the bigger of two numbers" challenge.
    pub captcha_pick_bigger_label: String,
    /// Shown after a wrong captcha answer. `{attempts}` is replaced with the
    /// number of attempts left.
    pub captcha_wrong_answer: String,
    /// Shown when a command is used before the captcha is solved.
    pub captcha_command_blocked: String,
    /// Kick message shown on success (the `[PLETX]` prefix is added by the code).
    pub captcha_success: String,
    /// Kick message shown after too many wrong answers.
    pub captcha_failed: String,
    /// Kick message shown when the captcha is not solved in time.
    pub captcha_timeout: String,
}

impl LanguageMessages {
    /// Renders the wrong-answer message with the remaining attempt count.
    pub fn wrong_answer(&self, attempts_left: u8) -> String {
        self.captcha_wrong_answer
            .replace("{attempts}", &attempts_left.to_string())
    }

    /// Fills any empty field from `defaults`, so partially-edited files keep working.
    fn fill_empty_from(&mut self, defaults: &Self) {
        fill(&mut self.welcome, &defaults.welcome);
        fill(&mut self.captcha_copy_label, &defaults.captcha_copy_label);
        fill(&mut self.captcha_solve_label, &defaults.captcha_solve_label);
        fill(
            &mut self.captcha_pick_bigger_label,
            &defaults.captcha_pick_bigger_label,
        );
        fill(&mut self.captcha_wrong_answer, &defaults.captcha_wrong_answer);
        fill(
            &mut self.captcha_command_blocked,
            &defaults.captcha_command_blocked,
        );
        fill(&mut self.captcha_success, &defaults.captcha_success);
        fill(&mut self.captcha_failed, &defaults.captcha_failed);
        fill(&mut self.captcha_timeout, &defaults.captcha_timeout);
    }
}

fn fill(field: &mut String, default: &str) {
    if field.trim().is_empty() {
        default.clone_into(field);
    }
}

/// Built-in captcha strings for a language, in field order:
/// `[copy, solve, pick_bigger, wrong, command_blocked, success, failed, timeout]`.
/// Returns `None` for languages without built-in translations.
fn builtin_strings(code: &str) -> Option<[&'static str; 8]> {
    let strings = match code {
        "en" => [
            "Type this number:",
            "Solve:",
            "Type the bigger number:",
            "Oops, wrong! Tries left: {attempts}",
            "Type the answer in chat first.",
            "Verified! Please reconnect.",
            "Captcha failed. Try again later.",
            "Captcha timed out. Try again.",
        ],
        "tr" => [
            "Bu sayıyı yaz:",
            "Çöz:",
            "Büyük olan sayıyı yaz:",
            "Yanlış. Kalan hak: {attempts}",
            "Önce cevabı sohbete yaz.",
            "Doğrulandı! Lütfen tekrar bağlan.",
            "Doğrulama başarısız. Sonra tekrar dene.",
            "Süre doldu. Tekrar dene.",
        ],
        "de" => [
            "Tippe diese Zahl:",
            "Löse:",
            "Tippe die größere Zahl:",
            "Falsch. Noch {attempts} Versuche.",
            "Tippe zuerst die Antwort in den Chat.",
            "Bestätigt! Bitte neu verbinden.",
            "Captcha fehlgeschlagen. Versuche es später erneut.",
            "Zeit abgelaufen. Versuche es erneut.",
        ],
        "ar" => [
            "اكتب هذا الرقم:",
            "احسب:",
            "اكتب الرقم الأكبر:",
            "خطأ! تبقى لديك {attempts} محاولات.",
            "اكتب الإجابة في الدردشة أولاً.",
            "تم التحقق! يرجى إعادة الاتصال.",
            "فشل التحقق. حاول لاحقًا.",
            "انتهى الوقت. حاول مرة أخرى.",
        ],
        "nl" => [
            "Typ dit getal:",
            "Los op:",
            "Typ het grootste getal:",
            "Fout. Pogingen over: {attempts}",
            "Typ eerst het antwoord in de chat.",
            "Geverifieerd! Maak opnieuw verbinding.",
            "Captcha mislukt. Probeer het later opnieuw.",
            "Tijd verstreken. Probeer opnieuw.",
        ],
        "fr" => [
            "Tape ce nombre :",
            "Calcule :",
            "Tape le plus grand nombre :",
            "Faux. Essais restants : {attempts}",
            "Tape d'abord la réponse dans le chat.",
            "Vérifié ! Reconnecte-toi.",
            "Captcha échoué. Réessaie plus tard.",
            "Temps écoulé. Réessaie.",
        ],
        "es" => [
            "Escribe este número:",
            "Resuelve:",
            "Escribe el número mayor:",
            "¡Incorrecto! Intentos: {attempts}",
            "Primero escribe la respuesta en el chat.",
            "¡Verificado! Vuelve a conectarte.",
            "No se pudo verificar. Inténtalo más tarde.",
            "Tiempo agotado. Inténtalo de nuevo.",
        ],
        "it" => [
            "Scrivi questo numero:",
            "Risolvi:",
            "Scrivi il numero più grande:",
            "Sbagliato! Tentativi rimasti: {attempts}",
            "Scrivi prima la risposta in chat.",
            "Verificato! Riconnettiti.",
            "Captcha fallito. Riprova più tardi.",
            "Tempo scaduto. Riprova.",
        ],
        _ => return None,
    };
    Some(strings)
}

/// Built-in join welcome (shown as a full-screen title), per language. The
/// welcome lives entirely in the translation files: this is only the default
/// written out on first run. Unknown (custom) codes fall back to the English
/// welcome until translated.
fn builtin_welcome(code: &str) -> &'static str {
    match code {
        "tr" => "PicoLimbo'ya hoş geldin!",
        "de" => "Willkommen bei PicoLimbo!",
        "ar" => "مرحباً بك في PicoLimbo!",
        "nl" => "Welkom bij PicoLimbo!",
        "fr" => "Bienvenue sur PicoLimbo !",
        "es" => "¡Bienvenido a PicoLimbo!",
        "it" => "Benvenuto su PicoLimbo!",
        // English and any custom language.
        _ => "Solve the current captcha to join!",
    }
}

/// The built-in defaults for a language. Unknown (custom) codes fall back to the
/// English strings, so a freshly-created custom file still has working captcha
/// text and welcome until it is translated.
fn builtin(code: &str) -> LanguageMessages {
    let s = builtin_strings(code)
        .or_else(|| builtin_strings("en"))
        .expect("English built-in strings are always present");
    LanguageMessages {
        welcome: builtin_welcome(code).to_owned(),
        captcha_copy_label: s[0].to_owned(),
        captcha_solve_label: s[1].to_owned(),
        captcha_pick_bigger_label: s[2].to_owned(),
        captcha_wrong_answer: s[3].to_owned(),
        captcha_command_blocked: s[4].to_owned(),
        captcha_success: s[5].to_owned(),
        captcha_failed: s[6].to_owned(),
        captcha_timeout: s[7].to_owned(),
    }
}

/// Loaded translations, keyed by language code.
#[derive(Clone, Debug)]
pub struct Translations {
    messages: HashMap<String, LanguageMessages>,
}

impl Default for Translations {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Translations {
    /// In-memory translations using only the compiled-in defaults (no files).
    pub fn builtin() -> Self {
        let messages = BUILTIN_LANGUAGES
            .iter()
            .map(|&code| (code.to_owned(), builtin(code)))
            .collect();
        Self { messages }
    }

    /// Writes any missing built-in language file, then loads **every** `*.toml`
    /// in `directory` (built-in and custom), keyed by the file's base name.
    /// Adding a language is just dropping a new file and restarting.
    pub fn load_or_create(directory: &Path) -> Result<Self, I18nError> {
        fs::create_dir_all(directory)?;

        // Create any missing built-in file with the compiled-in defaults.
        for &code in BUILTIN_LANGUAGES {
            let path = directory.join(format!("{code}.toml"));
            if !path.exists() {
                fs::write(&path, toml::to_string_pretty(&builtin(code))?)?;
            }
        }

        // Load every .toml file (built-in + custom) by its code (file stem).
        let mut messages = HashMap::new();
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            let Some(code) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_ascii_lowercase)
            else {
                continue;
            };

            let mut loaded = toml::from_str::<LanguageMessages>(&fs::read_to_string(&path)?)?;
            loaded.fill_empty_from(&builtin(&code));
            messages.insert(code, loaded);
        }

        // English must always exist as the ultimate fallback.
        messages
            .entry("en".to_owned())
            .or_insert_with(|| builtin("en"));

        Ok(Self { messages })
    }

    /// Returns the messages for an exact language code, if loaded.
    pub fn get_exact(&self, code: &str) -> Option<&LanguageMessages> {
        self.messages.get(code)
    }

    /// Returns the messages for `code`, falling back to English then any language.
    pub fn get(&self, code: &str) -> &LanguageMessages {
        self.messages
            .get(code)
            .or_else(|| self.messages.get("en"))
            .or_else(|| self.messages.values().next())
            .expect("at least one language is always loaded")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn generates_loads_and_fills_partial_files() {
        let dir = env::temp_dir().join("pico_i18n_loader_test");
        let _ = fs::remove_dir_all(&dir);

        // First run: generates every built-in file with its localized defaults.
        let first = Translations::load_or_create(&dir).unwrap();
        assert_eq!(first.get("tr").welcome, "PicoLimbo'ya hoş geldin!");
        assert_eq!(first.get("tr").captcha_copy_label, "Bu sayıyı yaz:");
        assert!(dir.join("es.toml").exists());
        assert!(dir.join("it.toml").exists());

        // Operator edits tr.toml, leaving most keys out.
        fs::write(dir.join("tr.toml"), "welcome = \"Hoş geldin!\"\n").unwrap();
        // A brand-new custom language is dropped in.
        fs::write(
            dir.join("pt.toml"),
            "welcome = \"Bem-vindo!\"\ncaptcha_copy_label = \"Digite este número:\"\n",
        )
        .unwrap();

        // Reload (= server restart): edits applied, missing keys filled, custom
        // language loaded (its missing keys fall back to English).
        let second = Translations::load_or_create(&dir).unwrap();
        assert_eq!(second.get("tr").welcome, "Hoş geldin!");
        assert_eq!(
            second.get("tr").captcha_success,
            "Doğrulandı! Lütfen tekrar bağlan."
        );
        assert!(second.get_exact("pt").is_some(), "custom language loaded");
        assert_eq!(second.get("pt").welcome, "Bem-vindo!");
        assert_eq!(second.get("pt").captcha_copy_label, "Digite este número:");
        // Missing custom key falls back to English.
        assert_eq!(second.get("pt").captcha_solve_label, "Solve:");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_answer_substitutes_attempt_count() {
        let messages = Translations::builtin().get("en").clone();
        assert_eq!(messages.wrong_answer(2), "Oops, wrong! Tries left: 2");
    }

    #[test]
    fn welcome_is_localized_per_language() {
        let translations = Translations::builtin();
        assert_eq!(translations.get("en").welcome, "Solve the current captcha to join!");
        assert_eq!(translations.get("de").welcome, "Löse das aktuelle Captcha, um beizutreten!");
        // A custom (file-only) language falls back to the English welcome.
        assert_eq!(translations.get("xx").welcome, "Solve the current captcha to join!");
    }

    #[test]
    fn spanish_and_italian_are_built_in() {
        let translations = Translations::builtin();
        assert!(translations.get_exact("es").is_some());
        assert!(translations.get_exact("it").is_some());
        assert_eq!(translations.get("es").captcha_copy_label, "Escribe este número:");
        assert_eq!(translations.get("it").captcha_copy_label, "Scrivi questo numero:");
    }
}
