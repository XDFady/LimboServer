use minecraft_protocol::prelude::*;

/// Client Information sent by the client in both the configuration and play
/// states. Only the locale is decoded; the remaining fields (view distance, chat
/// settings, skin parts, ...) are version-dependent and not needed here, so they
/// are left in the buffer.
///
/// Note: this is wired up via the `minecraft:client_information` packet name,
/// which only exists in the generated reports for Minecraft 1.21+. Older clients
/// also send a Client Settings packet with a leading locale string, but the
/// trimmed reports for those versions do not include it, so locale is not
/// captured pre-1.21 and the caller falls back to the configured language.
#[derive(PacketIn)]
pub struct ClientInformationPacket {
    /// The client's locale, e.g. `en_us`, `tr_tr`, `fr_ca`. Max length 16.
    locale: String,
}

impl ClientInformationPacket {
    pub fn locale(&self) -> &str {
        &self.locale
    }
}
