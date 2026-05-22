pub mod captcha;
pub mod mirror_status;

use captcha::CaptchaOptions;
use mirror_status::MirrorStatus;

#[derive(Clone, Default)]
pub struct CustomOptions {
    pub captcha: CaptchaOptions,
    pub mirror_status: Option<MirrorStatus>,
}