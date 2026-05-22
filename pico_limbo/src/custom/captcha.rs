use crate::handlers::configuration::send_message;
use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;

use minecraft_protocol::prelude::ProtocolVersion;
use pico_text_component::prelude::{Component, MiniMessageError, parse_mini_message};
use rand::RngExt;

#[derive(Clone)]
pub struct CaptchaOptions {
    pub enabled: bool,
    pub success_kick_message: String,
    pub failed_kick_message: String,
    pub timeout_kick_message: String,
    pub max_attempts: u8,
    pub timeout_seconds: u64,
}

impl Default for CaptchaOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            success_kick_message: "Verification successful. Please reconnect.".to_string(),
            failed_kick_message: "Captcha failed. Please try again later.".to_string(),
            timeout_kick_message: "Captcha timed out. Please try again.".to_string(),
            max_attempts: 3,
            timeout_seconds: 60,
        }
    }
}

struct GeneratedCaptcha {
    prompt: Component,
    answer: String,
}

pub fn normalize_answer(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn random_template(prompt_body: &str) -> String {
    let mut rng = rand::rng();

    let templates = [
        format!(
            "<dark_gray>--------------------</dark_gray>\n<yellow>Human check</yellow>\n{prompt_body}\n<gray>Type the answer in normal chat. Do not use /.</gray>\n<dark_gray>--------------------</dark_gray>"
        ),
        format!(
            "<gold><bold>Verification Required</bold></gold>\n{prompt_body}\n<dark_gray>Only the final answer is accepted.</dark_gray>"
        ),
        format!(
            "<gray>[AntiBot]</gray> <yellow>Solve this to continue:</yellow>\n{prompt_body}\n<red>Commands are blocked until verified.</red>"
        ),
        format!(
            "<aqua>Security Check</aqua>\n{prompt_body}\n<gray>You have limited attempts.</gray>"
        ),
    ];

    templates[rng.random_range(0..templates.len())].clone()
}

fn generate_number_challenge() -> Result<GeneratedCaptcha, MiniMessageError> {
    let mut rng = rand::rng();

    let number = rng.random_range(10000..99999).to_string();

    let fake = rng.random_range(10000..99999).to_string();

    let bodies = [
        format!(
            "<gray>Type this number:</gray> <green><bold>{number}</bold></green>\n<dark_gray>Ignore this fake code: {fake}</dark_gray>"
        ),
        format!(
            "<gray>Answer with the green code only:</gray>\n<green><bold>{number}</bold></green> <dark_gray>{fake}</dark_gray>"
        ),
        format!(
            "<gray>Copy the real code:</gray> <green>{number}</green>\n<red>Do not type:</red> <dark_gray>{fake}</dark_gray>"
        ),
    ];

    let body = bodies[rng.random_range(0..bodies.len())].clone();

    Ok(GeneratedCaptcha {
        prompt: parse_mini_message(&random_template(&body))?,
        answer: normalize_answer(&number),
    })
}

fn generate_math_challenge() -> Result<GeneratedCaptcha, MiniMessageError> {
    let mut rng = rand::rng();

    let a = rng.random_range(3..25);
    let b = rng.random_range(2..18);
    let c = rng.random_range(1..10);

    let mode = rng.random_range(0..4);

    let (question, answer) = match mode {
        0 => (format!("{a} + {b}"), a + b),
        1 => (format!("{a} + {b} - {c}"), a + b - c),
        2 => (format!("{a} × {b}"), a * b),
        _ => {
            let answer = a + c;
            (format!("{} - {}", answer + b, b), answer)
        }
    };

    let fake_answer = answer + rng.random_range(2..8);

    let bodies = [
        format!(
            "<gray>Solve:</gray> <yellow><bold>{question}</bold></yellow>\n<dark_gray>Fake answer: {fake_answer}</dark_gray>"
        ),
        format!(
            "<gray>Math check:</gray> <gold>{question}</gold>\n<gray>Type only the number result.</gray>"
        ),
        format!(
            "<yellow>{question}</yellow>\n<gray>Send the result in chat.</gray>"
        ),
    ];

    let body = bodies[rng.random_range(0..bodies.len())].clone();

    Ok(GeneratedCaptcha {
        prompt: parse_mini_message(&random_template(&body))?,
        answer: normalize_answer(&answer.to_string()),
    })
}

fn generate_reverse_challenge() -> Result<GeneratedCaptcha, MiniMessageError> {
    let mut rng = rand::rng();

    let charset = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut text = String::new();

    for _ in 0..4 {
        let index = rng.random_range(0..charset.len());
        text.push(charset[index] as char);
    }

    let answer: String = text.chars().rev().collect();

    let bodies = [
        format!(
            "<gray>Type this code backwards:</gray> <aqua><bold>{text}</bold></aqua>\n<dark_gray>Example: ABCD becomes DCBA</dark_gray>"
        ),
        format!(
            "<gray>Reverse challenge:</gray> <aqua>{text}</aqua>\n<gray>Answer with the reversed text.</gray>"
        ),
    ];

    let body = bodies[rng.random_range(0..bodies.len())].clone();

    Ok(GeneratedCaptcha {
        prompt: parse_mini_message(&random_template(&body))?,
        answer: normalize_answer(&answer),
    })
}

fn generate_word_pick_challenge() -> Result<GeneratedCaptcha, MiniMessageError> {
    let mut rng = rand::rng();

    let words = [
        "apple", "stone", "river", "cloud", "torch", "pixel", "grass", "melon",
    ];

    let target = words[rng.random_range(0..words.len())];
    let fake1 = words[rng.random_range(0..words.len())];
    let fake2 = words[rng.random_range(0..words.len())];

    let bodies = [
        format!(
            "<gray>Type the <green>GREEN</green> word only:</gray>\n<green><bold>{target}</bold></green> <red>{fake1}</red> <yellow>{fake2}</yellow>"
        ),
        format!(
            "<gray>Only send the word in green:</gray>\n<red>{fake1}</red> <green>{target}</green> <dark_gray>{fake2}</dark_gray>"
        ),
    ];

    let body = bodies[rng.random_range(0..bodies.len())].clone();

    Ok(GeneratedCaptcha {
        prompt: parse_mini_message(&random_template(&body))?,
        answer: normalize_answer(target),
    })
}

fn generate_position_challenge() -> Result<GeneratedCaptcha, MiniMessageError> {
    let mut rng = rand::rng();

    let words = ["wolf", "cake", "iron", "sand", "leaf", "book"];
    let first = words[rng.random_range(0..words.len())];
    let second = words[rng.random_range(0..words.len())];
    let third = words[rng.random_range(0..words.len())];

    let mode = rng.random_range(0..3);

    let (position_text, answer) = match mode {
        0 => ("first", first),
        1 => ("second", second),
        _ => ("third", third),
    };

    let body = format!(
        "<gray>Type the <yellow>{position_text}</yellow> word:</gray>\n<aqua>{first}</aqua> <aqua>{second}</aqua> <aqua>{third}</aqua>"
    );

    Ok(GeneratedCaptcha {
        prompt: parse_mini_message(&random_template(&body))?,
        answer: normalize_answer(answer),
    })
}

fn generate_challenge() -> Result<GeneratedCaptcha, MiniMessageError> {
    let mut rng = rand::rng();

    match rng.random_range(0..5) {
        0 => generate_number_challenge(),
        1 => generate_math_challenge(),
        2 => generate_reverse_challenge(),
        3 => generate_word_pick_challenge(),
        _ => generate_position_challenge(),
    }
}

pub fn wrong_captcha_message(attempts_left: u8) -> Result<Component, MiniMessageError> {
    let mut rng = rand::rng();

    let templates = [
        format!(
            "<red>Wrong answer.</red> <gray>Attempts left: {attempts_left}</gray>"
        ),
        format!(
            "<yellow>That was not correct.</yellow> <gray>{attempts_left} attempt(s) left.</gray>"
        ),
        format!(
            "<red>Verification failed.</red> <dark_gray>Remaining: {attempts_left}</dark_gray>"
        ),
    ];

    parse_mini_message(&templates[rng.random_range(0..templates.len())])
}

pub fn command_blocked_message() -> Result<Component, MiniMessageError> {
    let mut rng = rand::rng();

    let templates = [
        "<yellow>Please solve the captcha in normal chat first.</yellow>",
        "<red>Commands are disabled until you finish verification.</red>",
        "<gray>Type the captcha answer directly in chat, without /.</gray>",
    ];

    parse_mini_message(templates[rng.random_range(0..templates.len())])
}

pub fn start_for_client(
    client_state: &mut ClientState,
    batch: &mut Batch<PacketRegistry>,
    protocol_version: ProtocolVersion,
) -> Result<(), String> {
    let challenge = generate_challenge().map_err(|err| err.to_string())?;

    client_state.start_captcha(challenge.answer);

    send_message(batch, &challenge.prompt, protocol_version);

    Ok(())
}

pub fn block_command_if_waiting(
    client_state: &ClientState,
    batch: &mut Batch<PacketRegistry>,
    protocol_version: ProtocolVersion,
) -> bool {
    if !client_state.is_waiting_for_captcha() {
        return false;
    }

    if let Ok(component) = command_blocked_message() {
        send_message(batch, &component, protocol_version);
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

    let input = normalize_answer(message);
    let captcha_options = &server_state.custom().captcha;

    if client_state.is_captcha_correct(&input) {
        client_state.mark_captcha_verified();
        client_state.kick(&captcha_options.success_kick_message);
        return true;
    }

    let attempts = client_state.add_captcha_attempt();
    let max_attempts = captcha_options.max_attempts.max(1);

    if attempts >= max_attempts {
        client_state.kick(&captcha_options.failed_kick_message);
        return true;
    }

    let attempts_left = max_attempts.saturating_sub(attempts);

    if let Ok(component) = wrong_captcha_message(attempts_left) {
        send_message(batch, &component, client_state.protocol_version());
    }

    true
}

pub fn kick_if_expired(
    client_state: &mut ClientState,
    server_state: &ServerState,
) {
    if !server_state.custom().captcha.enabled {
        return;
    }

    if !client_state.is_waiting_for_captcha() {
        return;
    }

    let timeout_seconds = server_state.custom().captcha.timeout_seconds;

    if client_state.has_captcha_timed_out(timeout_seconds) {
        client_state.kick(&server_state.custom().captcha.timeout_kick_message);
    }
}