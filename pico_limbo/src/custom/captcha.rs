use crate::handlers::configuration::send_message;
use crate::i18n::LanguageMessages;
use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;

use pico_text_component::prelude::{Component, MiniMessageError, parse_mini_message};
use rand::RngExt;

/// Brand prefix prepended to the success (reconnect) message.
const BRAND_PREFIX: &str = "[PLETX]";

#[derive(Clone)]
pub struct CaptchaOptions {
    pub enabled: bool,
    /// Optional override for the success message. Empty = use the built-in
    /// translation for the player's language. The `[PLETX]` prefix is always added.
    pub success_kick_message: String,
    /// Optional override for the failure message. Empty = use the translation.
    pub failed_kick_message: String,
    /// Optional override for the timeout message. Empty = use the translation.
    pub timeout_kick_message: String,
    pub max_attempts: u8,
    pub timeout_seconds: u64,
}

impl Default for CaptchaOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            // Empty => use the built-in translated message for the player's language.
            success_kick_message: String::new(),
            failed_kick_message: String::new(),
            timeout_kick_message: String::new(),
            max_attempts: 3,
            timeout_seconds: 60,
        }
    }
}

/// A simple, kid-friendly captcha challenge with a numeric answer. Numeric
/// answers keep the challenge language-independent: only the short instruction
/// label is translated. The variety of formats means a bot cannot pattern-match a
/// single prompt shape, while each one is trivial for a human.
enum Challenge {
    /// Copy the shown number.
    Copy(u32),
    /// Add two small numbers.
    Add(u32, u32),
    /// Subtract (result is always positive).
    Subtract(u32, u32),
    /// Multiply two tiny numbers.
    Multiply(u32, u32),
    /// Type the bigger of two numbers.
    Bigger(u32, u32),
}

impl Challenge {
    fn random() -> Self {
        let mut rng = rand::rng();
        match rng.random_range(0..5) {
            0 => Self::Copy(rng.random_range(1000..10000)),
            1 => Self::Add(rng.random_range(1..10), rng.random_range(1..10)),
            2 => {
                let a = rng.random_range(6..20);
                let b = rng.random_range(1..a);
                Self::Subtract(a, b)
            }
            3 => Self::Multiply(rng.random_range(2..6), rng.random_range(2..6)),
            _ => {
                let a = rng.random_range(1..50);
                let mut b = rng.random_range(1..50);
                if b == a {
                    b = if a > 1 { a - 1 } else { a + 1 };
                }
                Self::Bigger(a, b)
            }
        }
    }

    /// The expected (normalized) answer.
    fn answer(&self) -> String {
        match self {
            Self::Copy(number) => number.to_string(),
            Self::Add(a, b) => (a + b).to_string(),
            Self::Subtract(a, b) => (a - b).to_string(),
            Self::Multiply(a, b) => (a * b).to_string(),
            Self::Bigger(a, b) => (*a).max(*b).to_string(),
        }
    }

    /// Builds the minimal, localized prompt component.
    fn prompt(&self, messages: &LanguageMessages) -> Result<Component, MiniMessageError> {
        let body = match self {
            Self::Copy(number) => format!(
                "<yellow>{}</yellow> <green><bold>{number}</bold></green>",
                messages.captcha_copy_label
            ),
            Self::Add(a, b) => format!(
                "<yellow>{}</yellow> <green><bold>{a} + {b}</bold></green>",
                messages.captcha_solve_label
            ),
            Self::Subtract(a, b) => format!(
                "<yellow>{}</yellow> <green><bold>{a} - {b}</bold></green>",
                messages.captcha_solve_label
            ),
            Self::Multiply(a, b) => format!(
                "<yellow>{}</yellow> <green><bold>{a} × {b}</bold></green>",
                messages.captcha_solve_label
            ),
            Self::Bigger(a, b) => format!(
                "<yellow>{}</yellow> <green><bold>{a}   {b}</bold></green>",
                messages.captcha_pick_bigger_label
            ),
        };
        parse_mini_message(&body)
    }
}

pub fn normalize_answer(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn success_message(messages: &LanguageMessages, options: &CaptchaOptions) -> String {
    let base = if options.success_kick_message.trim().is_empty() {
        messages.captcha_success.clone()
    } else {
        options.success_kick_message.clone()
    };
    format!("{BRAND_PREFIX} {base}")
}

fn failed_message(messages: &LanguageMessages, options: &CaptchaOptions) -> String {
    if options.failed_kick_message.trim().is_empty() {
        messages.captcha_failed.clone()
    } else {
        options.failed_kick_message.clone()
    }
}

fn timeout_message(messages: &LanguageMessages, options: &CaptchaOptions) -> String {
    if options.timeout_kick_message.trim().is_empty() {
        messages.captcha_timeout.clone()
    } else {
        options.timeout_kick_message.clone()
    }
}

pub fn start_for_client(
    client_state: &mut ClientState,
    server_state: &ServerState,
    batch: &mut Batch<PacketRegistry>,
) -> Result<(), String> {
    let messages = server_state.resolve_messages(client_state.locale());
    let protocol_version = client_state.protocol_version();
    let challenge = Challenge::random();
    let prompt = challenge.prompt(messages).map_err(|err| err.to_string())?;

    client_state.start_captcha(normalize_answer(&challenge.answer()));
    send_message(batch, &prompt, protocol_version);

    Ok(())
}

pub fn block_command_if_waiting(
    client_state: &ClientState,
    server_state: &ServerState,
    batch: &mut Batch<PacketRegistry>,
) -> bool {
    if !client_state.is_waiting_for_captcha() {
        return false;
    }

    let messages = server_state.resolve_messages(client_state.locale());
    if let Ok(component) = parse_mini_message(&format!(
        "<yellow>{}</yellow>",
        messages.captcha_command_blocked
    )) {
        send_message(batch, &component, client_state.protocol_version());
    }

    true
}

pub fn handle_chat_message(
    client_state: &mut ClientState,
    server_state: &ServerState,
    message: &str,
    batch: &mut Batch<PacketRegistry>,
) -> bool {
    if !client_state.is_waiting_for_captcha() {
        return false;
    }

    let messages = server_state.resolve_messages(client_state.locale());
    let options = &server_state.custom().captcha;
    let input = normalize_answer(message);

    if client_state.is_captcha_correct(&input) {
        client_state.mark_captcha_verified();
        client_state.kick(&success_message(messages, options));
        return true;
    }

    let attempts = client_state.add_captcha_attempt();
    let max_attempts = options.max_attempts.max(1);

    if attempts >= max_attempts {
        client_state.kick(&failed_message(messages, options));
        return true;
    }

    let attempts_left = max_attempts.saturating_sub(attempts);
    if let Ok(component) = parse_mini_message(&format!(
        "<red>{}</red>",
        messages.wrong_answer(attempts_left)
    )) {
        send_message(batch, &component, client_state.protocol_version());
    }

    true
}

pub fn kick_if_expired(client_state: &mut ClientState, server_state: &ServerState) {
    let options = &server_state.custom().captcha;
    if !options.enabled || !client_state.is_waiting_for_captcha() {
        return;
    }

    if client_state.has_captcha_timed_out(options.timeout_seconds) {
        let messages = server_state.resolve_messages(client_state.locale());
        client_state.kick(&timeout_message(messages, options));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{BUILTIN_LANGUAGES, Translations};

    /// Exercises every challenge variant against every built-in language to make
    /// sure each localized prompt parses as valid `MiniMessage`.
    #[test]
    fn challenges_and_messages_render_for_every_language() {
        let translations = Translations::builtin("");
        let challenges = [
            Challenge::Copy(1234),
            Challenge::Add(2, 3),
            Challenge::Subtract(9, 4),
            Challenge::Multiply(2, 3),
            Challenge::Bigger(8, 3),
        ];
        for &code in BUILTIN_LANGUAGES {
            let messages = translations.get(code);
            for challenge in &challenges {
                challenge
                    .prompt(messages)
                    .unwrap_or_else(|err| panic!("prompt failed for {code}: {err}"));
            }
            parse_mini_message(&format!("<yellow>{}</yellow>", messages.captcha_command_blocked))
                .expect("command blocked message must parse");
            parse_mini_message(&format!("<red>{}</red>", messages.wrong_answer(2)))
                .expect("wrong answer message must parse");
        }
    }

    #[test]
    fn success_message_is_always_prefixed() {
        let translations = Translations::builtin("");
        let options = CaptchaOptions::default();
        for &code in BUILTIN_LANGUAGES {
            assert!(success_message(translations.get(code), &options).starts_with("[PLETX] "));
        }
        // Operator override is also prefixed.
        let overridden = CaptchaOptions {
            success_kick_message: "Custom done".to_string(),
            ..CaptchaOptions::default()
        };
        assert_eq!(
            success_message(translations.get("en"), &overridden),
            "[PLETX] Custom done"
        );
    }

    #[test]
    fn challenge_answers_are_correct() {
        assert_eq!(Challenge::Copy(1234).answer(), "1234");
        assert_eq!(Challenge::Add(2, 3).answer(), "5");
        assert_eq!(Challenge::Subtract(9, 4).answer(), "5");
        assert_eq!(Challenge::Multiply(2, 3).answer(), "6");
        assert_eq!(Challenge::Bigger(8, 3).answer(), "8");
        assert_eq!(Challenge::Bigger(3, 8).answer(), "8");
    }

    #[tokio::test]
    async fn non_ascii_kick_survives_wire_codec() {
        use minecraft_packets::play::disconnect_packet::DisconnectPacket;
        use minecraft_protocol::prelude::ProtocolVersion;
        use net::packet_stream::PacketStream;

        let reason = "[PLETX] Doğrulandı! Lütfen tekrar bağlan. مرحبا";
        let raw = PacketRegistry::PlayDisconnect(DisconnectPacket::text(reason))
            .encode_packet(ProtocolVersion::V1_21)
            .unwrap();
        let expected = raw.bytes().to_vec();
        eprintln!("V1_21 kick packet ({} bytes): {raw}", raw.size());

        // Round-trip through the real wire codec (uncompressed).
        let (a, b) = tokio::io::duplex(8192);
        let mut writer = PacketStream::new(a);
        let mut reader = PacketStream::new(b);
        writer.write_packet(raw).await.unwrap();
        let back = reader.read_packet().await.unwrap();
        assert_eq!(back.bytes(), expected.as_slice(), "wire round-trip changed bytes");

        // The exact UTF-8 bytes of the reason must appear intact inside the packet
        // body (the NBT string is byte-length-prefixed, standard UTF-8).
        let haystack = back.bytes();
        let needle = reason.as_bytes();
        assert!(
            haystack.windows(needle.len()).any(|w| w == needle),
            "reason bytes were not embedded intact in the packet"
        );
    }

    #[test]
    fn non_ascii_kick_and_prompt_encode_for_all_versions() {
        use minecraft_packets::play::disconnect_packet::DisconnectPacket;
        use minecraft_protocol::prelude::ProtocolVersion;

        let reason = "[PLETX] Doğrulandı! Lütfen tekrar bağlan. مرحبا";
        let versions = [
            ProtocolVersion::V1_8,
            ProtocolVersion::V1_16,
            ProtocolVersion::V1_19,
            ProtocolVersion::V1_20_2,
            ProtocolVersion::V1_20_3,
            ProtocolVersion::V1_21,
            ProtocolVersion::V1_21_5,
        ];

        for pv in versions {
            let kick = PacketRegistry::PlayDisconnect(DisconnectPacket::text(reason));
            let encoded = kick.encode_packet(pv);
            assert!(
                encoded.is_ok(),
                "kick encode failed for {pv:?}: {:?}",
                encoded.err()
            );

            let translations = Translations::builtin("");
            let prompt = Challenge::Copy(1234)
                .prompt(translations.get("tr"))
                .unwrap();
            let mut batch = Batch::new();
            send_message(&mut batch, &prompt, pv);
        }
    }
}
