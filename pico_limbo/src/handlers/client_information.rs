use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::configuration::client_information_packet::ClientInformationPacket;
use minecraft_protocol::prelude::State;

impl PacketHandler for ClientInformationPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        server_state: &ServerState,
    ) -> Result<Batch<PacketRegistry>, PacketHandlerError> {
        let mut batch = Batch::new();

        // Capture the locale so the captcha/auth messages and welcome use the
        // player's language. Sent during configuration on 1.20.2+, and in play on
        // older versions (and whenever the player changes their language).
        client_state.set_locale(self.locale());

        // Versions before 1.20.2 have no configuration phase, so this packet
        // arrives in play state — after the join messages were already sent in the
        // fallback language. If the now-known locale changes the resolved language,
        // re-localize the welcome and captcha prompt so it takes effect on join.
        if client_state.state() == State::Play {
            let resolved = server_state.resolve_code(client_state.locale());
            if client_state.join_language() != Some(resolved.as_str()) {
                client_state.set_join_language(&resolved);
                crate::handlers::configuration::relocalize_join(
                    &mut batch,
                    client_state,
                    server_state,
                )?;
            }
        }

        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::CustomOptions;
    use crate::custom::captcha::CaptchaOptions;
    use futures::StreamExt;
    use minecraft_protocol::prelude::ProtocolVersion;
    use net::raw_packet::RawPacket;

    /// A serverbound Client Settings packet: leading `VarInt`-prefixed locale
    /// string (locale length < 128, so the `VarInt` length is a single byte).
    fn locale_packet(packet_id: u8, locale: &str) -> RawPacket {
        let mut fields = vec![u8::try_from(locale.len()).unwrap()];
        fields.extend_from_slice(locale.as_bytes());
        RawPacket::from_bytes(packet_id, &fields)
    }

    fn decode_ci_locale(
        pv: ProtocolVersion,
        state: State,
        packet_id: u8,
        locale: &str,
    ) -> ClientInformationPacket {
        let raw = locale_packet(packet_id, locale);
        match PacketRegistry::decode_packet(pv, state, raw) {
            Ok(
                PacketRegistry::PlayClientInformation(p)
                | PacketRegistry::ConfigurationClientInformation(p),
            ) => p,
            Ok(_) => panic!("decoded a different packet for {pv:?}/{state:?} id {packet_id}"),
            Err(e) => panic!("failed to decode ClientInformation for {pv:?}/{state:?}: {e}"),
        }
    }

    fn decode_ci(pv: ProtocolVersion, state: State, packet_id: u8) -> ClientInformationPacket {
        decode_ci_locale(pv, state, packet_id, "fr_fr")
    }

    /// Decodes a real Client Settings packet on every supported version using the
    /// serverbound IDs added to the reports, proving them end to end. Wrong IDs
    /// would match the wrong packet (or none) and fail here.
    #[test]
    fn client_information_decodes_on_every_version() {
        // (version, verified serverbound PLAY Client Settings id).
        let play = [
            (ProtocolVersion::V1_7_2, 21),
            (ProtocolVersion::V1_8, 21),
            (ProtocolVersion::V1_9, 4),
            (ProtocolVersion::V1_9_3, 4),
            (ProtocolVersion::V1_10, 4),
            (ProtocolVersion::V1_11, 4),
            (ProtocolVersion::V1_12, 5),
            (ProtocolVersion::V1_12_1, 4), // a packet removed in 1.12.1 shifted this down
            (ProtocolVersion::V1_13, 4),
            (ProtocolVersion::V1_14, 5),
            (ProtocolVersion::V1_15, 5),
            (ProtocolVersion::V1_16, 5),
            (ProtocolVersion::V1_16_2, 5),
            (ProtocolVersion::V1_17, 5),
            (ProtocolVersion::V1_18, 5),
            (ProtocolVersion::V1_18_2, 5),
            (ProtocolVersion::V1_19, 7),
            (ProtocolVersion::V1_19_1, 8),
            (ProtocolVersion::V1_19_3, 7),
            (ProtocolVersion::V1_19_4, 8),
            (ProtocolVersion::V1_20, 8),
            (ProtocolVersion::V1_20_2, 9),
            (ProtocolVersion::V1_20_3, 9),
            (ProtocolVersion::V1_20_5, 10),
            (ProtocolVersion::V1_21, 10), // modern (full report)
            // Real sub-versions that share a packets group must route to the same
            // id via `protocol_version.packets()`.
            (ProtocolVersion::V1_7_6, 21),  // -> V1_7_2
            (ProtocolVersion::V1_9_1, 4),   // -> V1_9
            (ProtocolVersion::V1_12_2, 4),  // -> V1_12_1
            (ProtocolVersion::V1_14_4, 5),  // -> V1_14
            (ProtocolVersion::V1_16_4, 5),  // -> V1_16_2
            (ProtocolVersion::V1_17_1, 5),  // -> V1_17
        ];
        for (pv, id) in play {
            assert_eq!(
                decode_ci(pv, State::Play, id).locale(),
                "fr_fr",
                "play {pv:?}"
            );
        }
        // Configuration state exists from 1.20.2; Client Information is id 0.
        for pv in [
            ProtocolVersion::V1_20_2,
            ProtocolVersion::V1_20_3,
            ProtocolVersion::V1_20_5,
            ProtocolVersion::V1_21,
        ] {
            assert_eq!(
                decode_ci(pv, State::Configuration, 0).locale(),
                "fr_fr",
                "config {pv:?}"
            );
        }
    }

    fn captcha_server() -> ServerState {
        let mut builder = ServerState::builder();
        builder
            .view_distance(0)
            .fallback_language("en".to_string())
            .custom(CustomOptions {
                captcha: CaptchaOptions {
                    enabled: true,
                    ..CaptchaOptions::default()
                },
                mirror_status: None,
            });
        builder.build().unwrap()
    }

    async fn count(batch: Batch<PacketRegistry>) -> usize {
        let mut stream = batch.into_stream();
        let mut n = 0;
        while stream.next().await.is_some() {
            n += 1;
        }
        n
    }

    /// On an old version, a locale that arrives in play state after join and that
    /// changes the resolved language re-localizes; the same language does not.
    #[tokio::test]
    async fn relocalizes_only_when_late_locale_changes_language() {
        let server_state = captcha_server();

        // Captcha already shown in the fallback language (en).
        let mut client_state = ClientState::default();
        client_state.set_protocol_version(ProtocolVersion::V1_8);
        client_state.set_state(State::Play);
        client_state.set_join_language("en");
        client_state.start_captcha("1234".to_string());

        // Turkish locale arrives -> language changes -> re-localize.
        let tr = decode_ci_locale(ProtocolVersion::V1_8, State::Play, 21, "tr_tr");
        let batch = tr.handle(&mut client_state, &server_state).unwrap();
        assert_eq!(client_state.join_language(), Some("tr"));
        assert!(count(batch).await > 0, "expected re-localize packets");

        // Same language (en_us == fallback en): no re-localize.
        let mut same = ClientState::default();
        same.set_protocol_version(ProtocolVersion::V1_8);
        same.set_state(State::Play);
        same.set_join_language("en");
        same.start_captcha("1234".to_string());
        let en = decode_ci_locale(ProtocolVersion::V1_8, State::Play, 21, "en_us");
        let batch = en.handle(&mut same, &server_state).unwrap();
        assert_eq!(same.join_language(), Some("en"));
        assert_eq!(count(batch).await, 0, "no re-localize when language unchanged");
    }
}
