use crate::server::game_profile::GameProfile;
use minecraft_packets::login::Property;
use minecraft_protocol::prelude::{ProtocolVersion, State, Uuid};
use tracing::debug;
use std::time::{Duration, Instant};

#[derive(PartialEq, Eq)]
pub enum KeepAliveStatus {
    Disabled,
    ShouldEnable,
    Enabled,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            state: State::Handshake,
            protocol_version: ProtocolVersion::Any,
            kick_message: None,
            message_id: -1,
            game_profile: None,
            keep_alive_enabled: KeepAliveStatus::Disabled,
            feet_y: 0.0,
            is_flight_allowed: false,
            is_flying: false,
            flying_speed: 0.05,
            captcha_code: None,
            captcha_attempts: 0,
            captcha_verified: false,
            captcha_started_at: None,
            locale: None,
            join_language: None,
        }
    }
}

pub struct ClientState {
    state: State,
    protocol_version: ProtocolVersion,
    kick_message: Option<String>,
    message_id: i32,
    game_profile: Option<GameProfile>,
    keep_alive_enabled: KeepAliveStatus,
    feet_y: f64,
    is_flight_allowed: bool,
    is_flying: bool,
    flying_speed: f32,
    captcha_code: Option<String>,
    captcha_attempts: u8,
    captcha_verified: bool,
    captcha_started_at: Option<Instant>,
    locale: Option<String>,
    join_language: Option<String>,
}

impl ClientState {
    const ANONYMOUS: &'static str = "Anonymous";

    // Kick

    pub fn kick(&mut self, kick_message: &str) {
        self.kick_message = Some(kick_message.to_string());
    }

    pub fn should_kick(&self) -> Option<String> {
        self.kick_message.clone()
    }

    // State

    pub const fn state(&self) -> State {
        self.state
    }

    pub const fn set_state(&mut self, new_state: State) {
        self.state = new_state;
    }

    // Protocol version

    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    pub const fn set_protocol_version(&mut self, new_protocol_version: ProtocolVersion) {
        self.protocol_version = new_protocol_version;
    }

    // Velocity

    pub const fn set_velocity_login_message_id(&mut self, message_id: i32) {
        self.message_id = message_id;
    }

    pub const fn get_velocity_login_message_id(&self) -> i32 {
        self.message_id
    }

    // Game profile

    pub fn set_game_profile(&mut self, game_profile: GameProfile) {
        if let Some(ref mut existing_game_profile) = self.game_profile {
            existing_game_profile.set_name(&game_profile.username());
        } else {
            self.game_profile = Some(game_profile);
        }

        if let Some(ref existing_game_profile) = self.game_profile
            && !existing_game_profile.is_anonymous()
        {
            // Per-connection log: only at verbose (`-v`) level, to avoid console
            // spam when many connections arrive per second.
            debug!(
                "UUID of player {} is {}",
                existing_game_profile.username(),
                existing_game_profile.uuid()
            );
        }
    }

    pub fn game_profile(&self) -> Option<GameProfile> {
        self.game_profile.clone()
    }

    pub fn get_username(&self) -> String {
        self.game_profile().map_or_else(
            || Self::ANONYMOUS.to_owned(),
            |profile| profile.username().to_owned(),
        )
    }

    pub fn get_unique_id(&self) -> Uuid {
        self.game_profile()
            .map_or_else(Uuid::default, |profile| profile.uuid())
    }

    pub fn get_textures(&self) -> Option<Property> {
        self.game_profile()
            .and_then(|profile| profile.textures().cloned())
    }

    // Keep alive

    pub fn should_enable_keep_alive(&self) -> bool {
        self.keep_alive_enabled == KeepAliveStatus::ShouldEnable
    }

    pub fn set_keep_alive_should_enable(&mut self) {
        if self.keep_alive_enabled == KeepAliveStatus::Disabled {
            self.keep_alive_enabled = KeepAliveStatus::ShouldEnable;
        }
    }

    pub fn set_keep_alive_enabled(&mut self) {
        if self.keep_alive_enabled == KeepAliveStatus::ShouldEnable {
            self.keep_alive_enabled = KeepAliveStatus::Enabled;
        }
    }

    // Position

    pub const fn get_y_position(&self) -> f64 {
        self.feet_y
    }

    pub const fn set_feet_position(&mut self, feet_y: f64) {
        self.feet_y = feet_y;
    }

    // Movement

    pub const fn is_flight_allowed(&self) -> bool {
        self.is_flight_allowed
    }

    pub const fn set_is_flight_allowed(&mut self, allow_flight: bool) {
        self.is_flight_allowed = allow_flight;
    }

    pub const fn is_flying(&self) -> bool {
        self.is_flying
    }

    pub const fn set_is_flying(&mut self, is_flying: bool) {
        self.is_flying = is_flying;
    }

    pub const fn get_flying_speed(&self) -> f32 {
        self.flying_speed
    }

    pub const fn set_flying_speed(&mut self, flying_speed: f32) {
        self.flying_speed = flying_speed;
    }

    // Custom captcha
    pub fn start_captcha(&mut self, code: String) {
        self.captcha_code = Some(code);
        self.captcha_attempts = 0;
        self.captcha_verified = false;
        self.captcha_started_at = Some(Instant::now());
    }

    pub fn is_waiting_for_captcha(&self) -> bool {
        self.captcha_code.is_some() && !self.captcha_verified
    }

    pub fn is_captcha_correct(&self, input: &str) -> bool {
        self.captcha_code.as_deref() == Some(input)
    }

    pub fn add_captcha_attempt(&mut self) -> u8 {
        self.captcha_attempts = self.captcha_attempts.saturating_add(1);
        self.captcha_attempts
    }

    pub fn mark_captcha_verified(&mut self) {
        self.captcha_verified = true;
        self.captcha_code = None;
        self.captcha_started_at = None;
    }

    pub fn has_captcha_timed_out(&self, timeout_seconds: u64) -> bool {
        if !self.is_waiting_for_captcha() {
            return false;
        }

        let Some(started_at) = self.captcha_started_at else {
            return false;
        };

        started_at.elapsed() >= Duration::from_secs(timeout_seconds)
    }

    // Locale (client language)

    /// Stores the client's locale (e.g. `tr_tr`), reported by the Client
    /// Information packet. Empty values are ignored.
    pub fn set_locale(&mut self, locale: &str) {
        if !locale.trim().is_empty() {
            self.locale = Some(locale.to_owned());
        }
    }

    pub fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }

    /// Language code the join messages (welcome/captcha) were last rendered in.
    /// Used to detect when a late-arriving locale changes the language so the
    /// messages can be re-localized.
    pub fn set_join_language(&mut self, code: &str) {
        self.join_language = Some(code.to_owned());
    }

    pub fn join_language(&self) -> Option<&str> {
        self.join_language.as_deref()
    }
}
