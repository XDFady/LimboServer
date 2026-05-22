use clap::Parser;
use std::path::PathBuf;
use crate::custom::captcha::CaptchaOptions;
use crate::custom::mirror_status::MirrorStatus;
use crate::custom::CustomOptions;

#[derive(Parser, Clone)]
#[command(
    about = "A lightweight Minecraft server written in Rust supporting all Minecraft versions"
)]
pub struct Cli {
    /// Enable verbose logging
    #[arg(
        short = 'v',
        long = "verbose",
        action = clap::ArgAction::Count,
        help = "Enable verbose logging (-v for debug, -vv for trace)"
    )]
    pub verbose: u8,

    /// Path to the TOML configuration file
    #[arg(
        short = 'c',
        long = "config",
        value_name = "CONFIG_PATH",
        default_value = "server.toml",
        help = "Configuration file path"
    )]
    pub config_path: PathBuf,

    /// Enable the custom captcha module
    #[arg(
        long = "captcha",
        help = "Require players to type a captcha in normal chat before being kicked with the success message"
    )]
    pub captcha: bool,

    /// Message used when the player enters the captcha correctly
    #[arg(
        long = "success-kick-message",
        value_name = "MESSAGE",
        default_value = "Verification successful. Please reconnect."
    )]
    pub success_kick_message: String,

    /// Message used when the player fails the captcha
    #[arg(
        long = "failed-kick-message",
        value_name = "MESSAGE",
        default_value = "Captcha failed. Please try again later."
    )]
    pub failed_kick_message: String,

    /// Maximum wrong captcha attempts before kicking the player
    #[arg(
        long = "captcha-max-attempts",
        value_name = "COUNT",
        default_value_t = 3
    )]
    pub captcha_max_attempts: u8,

    /// Server to mirror in the server list, example: play.example.com:25565
    #[arg(
        long = "mirror-server",
        value_name = "HOST:PORT",
        help = "Minecraft server address used for mirroring MOTD and player count"
    )]
    pub mirror_server: Option<String>,

    /// How often to refresh mirrored server-list data
    #[arg(
        long = "mirror-refresh-seconds",
        value_name = "SECONDS",
        default_value_t = 10
    )]
    pub mirror_refresh_seconds: u64,

    /// Timeout for each mirror ping
    #[arg(
        long = "mirror-timeout-seconds",
        value_name = "SECONDS",
        default_value_t = 3
    )]
    pub mirror_timeout_seconds: u64,

    /// Seconds before kicking a player who did not solve captcha
    #[arg(
        long = "captcha-timeout-seconds",
        value_name = "SECONDS",
        default_value_t = 60
    )]
    pub captcha_timeout_seconds: u64,

    /// Message used when the player does not solve captcha in time
    #[arg(
        long = "captcha-timeout-kick-message",
        value_name = "MESSAGE",
        default_value = "Captcha timed out. Please try again."
    )]
    pub captcha_timeout_kick_message: String,
}

impl Cli {
    pub fn custom_options(&self) -> CustomOptions {
        CustomOptions {
            captcha: CaptchaOptions {
                enabled: self.captcha,
                success_kick_message: self.success_kick_message.clone(),
                failed_kick_message: self.failed_kick_message.clone(),
                timeout_kick_message: self.captcha_timeout_kick_message.clone(),
                max_attempts: self.captcha_max_attempts,
                timeout_seconds: self.captcha_timeout_seconds,
            },
            mirror_status: self.mirror_server.as_ref().map(|target| {
                MirrorStatus::new(
                    target.clone(),
                    self.mirror_refresh_seconds,
                    self.mirror_timeout_seconds,
                )
            }),
        }
    }
}