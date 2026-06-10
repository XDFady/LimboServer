use crate::server::client_data::ClientData;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::{
    PacketRegistry, PacketRegistryDecodeError, PacketRegistryEncodeError,
};
use crate::server::shutdown_signal::shutdown_signal;
use crate::server_state::ServerState;
use futures::StreamExt;
use minecraft_packets::login::login_disconnect_packet::LoginDisconnectPacket;
use minecraft_packets::play::client_bound_keep_alive_packet::ClientBoundKeepAlivePacket;
use minecraft_packets::play::disconnect_packet::DisconnectPacket;
use minecraft_protocol::prelude::State;
use net::packet_stream::PacketStreamError;
use net::raw_packet::RawPacket;
use std::num::TryFromIntError;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

pub struct Server {
    state: Arc<RwLock<ServerState>>,
    listen_address: String,
}

impl Server {
    pub fn new(listen_address: &impl ToString, state: ServerState) -> Self {
        Self {
            state: Arc::new(RwLock::new(state)),
            listen_address: listen_address.to_string(),
        }
    }

    pub async fn run(self, token: Option<&CancellationToken>) {
        let listener = match TcpListener::bind(&self.listen_address).await {
            Ok(sock) => sock,
            Err(err) => {
                error!("Failed to bind to {}: {}", self.listen_address, err);
                std::process::exit(1);
            }
        };

        info!("Listening on: {}", self.listen_address);
        self.accept(&listener, token).await;
    }

    pub async fn accept(self, listener: &TcpListener, token: Option<&CancellationToken>) {
        loop {
            tokio::select! {
                 accept_result = listener.accept() => {
                    match accept_result {
                        Ok((socket, addr)) => {
                            debug!("Accepted connection from {}", addr);
                        let state_clone = Arc::clone(&self.state);
                            tokio::spawn(async move {
                                handle_client(socket, state_clone).await;
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept a connection: {:?}", e);
                        }
                    }
                },

                 () = shutdown_signal(token) => {
                    info!("Shutdown signal received, shutting down gracefully.");
                    break;
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum PacketProcessingError {
    #[error("Client disconnected")]
    Disconnected,

    #[error("Packet not found version={0} state={1} packet_id={2}")]
    DecodePacketError(i32, State, u8),

    #[error("{0}")]
    Custom(String),
}

impl From<PacketHandlerError> for PacketProcessingError {
    fn from(e: PacketHandlerError) -> Self {
        match e {
            PacketHandlerError::Custom(reason) => Self::Custom(reason),
            PacketHandlerError::InvalidState(reason, should_warn) => {
                if should_warn {
                    warn!("{reason}");
                } else {
                    debug!("{reason}");
                }
                Self::Disconnected
            }
        }
    }
}

impl From<PacketRegistryDecodeError> for PacketProcessingError {
    fn from(e: PacketRegistryDecodeError) -> Self {
        match e {
            PacketRegistryDecodeError::NoCorrespondingPacket(version, state, packet_id) => {
                Self::DecodePacketError(version, state, packet_id)
            }
            _ => Self::Custom(e.to_string()),
        }
    }
}

impl From<PacketRegistryEncodeError> for PacketProcessingError {
    fn from(e: PacketRegistryEncodeError) -> Self {
        Self::Custom(e.to_string())
    }
}

impl From<TryFromIntError> for PacketProcessingError {
    fn from(e: TryFromIntError) -> Self {
        Self::Custom(e.to_string())
    }
}

impl From<PacketStreamError> for PacketProcessingError {
    fn from(_: PacketStreamError) -> Self {
        // Any stream/framing error is unrecoverable in a length-prefixed protocol:
        // we cannot resynchronise mid-stream after a bad length, oversized packet,
        // failed decompression, or a dead socket. Retrying would also spin the read
        // loop at full CPU on a persistently broken stream and amplify logging under
        // a flood. Drop the connection immediately instead.
        Self::Disconnected
    }
}

async fn process_packet(
    client_data: &ClientData,
    server_state: &Arc<RwLock<ServerState>>,
    raw_packet: RawPacket,
    was_in_play_state: &mut bool,
) -> Result<(), PacketProcessingError> {
    let mut client_state = client_data.client().await;
    let protocol_version = client_state.protocol_version();
    let state = client_state.state();
    let decoded_packet = PacketRegistry::decode_packet(protocol_version, state, raw_packet)?;

    let batch = {
        let server_state_guard = server_state.read().await;
        decoded_packet.handle(&mut client_state, &server_state_guard)?
    };

    let protocol_version = client_state.protocol_version();
    let state = client_state.state();

    if !*was_in_play_state && state == State::Play {
        *was_in_play_state = true;
        server_state.write().await.increment();
        let username = client_state.get_username();
        debug!(
            "{} joined using version {}",
            username,
            protocol_version.humanize()
        );
        info!("{} joined the game", username,);
    }

    let mut stream = batch.into_stream();
    while let Some(pending_packet) = stream.next().await {
        let enable_compression = matches!(pending_packet, PacketRegistry::SetCompression(..));
        let raw_packet = pending_packet.encode_packet(protocol_version)?;
        client_data.write_packet(raw_packet).await?;
        if enable_compression
            && let Some(compression_settings) = server_state.read().await.compression_settings()
        {
            let mut packet_stream = client_data.stream().await;
            packet_stream
                .set_compression(compression_settings.threshold, compression_settings.level);
        }
    }

    {
        let server_state_guard = server_state.read().await;

        crate::custom::captcha::kick_if_expired(
            &mut client_state,
            &*server_state_guard,
        );
    }

    if let Some(reason) = client_state.should_kick() {
        drop(client_state);
        kick_client(client_data, reason.clone())
            .await
            .map_err(|_| PacketProcessingError::Disconnected)?;
        return Err(PacketProcessingError::Disconnected);
    }

    drop(client_state);
    client_data.enable_keep_alive_if_needed().await;

    Ok(())
}

/// Waits for the next event on a connection: an inbound packet, a keep-alive
/// tick, or the idle `deadline` elapsing.
///
/// Returns `Ok(true)` when an inbound packet was received (the caller should
/// reset its idle deadline), `Ok(false)` after a keep-alive tick, and
/// `Err(Disconnected)` when the idle `deadline` is reached.
async fn read(
    client_data: &ClientData,
    server_state: &Arc<RwLock<ServerState>>,
    was_in_play_state: &mut bool,
    deadline: Option<Instant>,
) -> Result<bool, PacketProcessingError> {
    tokio::select! {
        result = client_data.read_packet() => {
            let raw_packet = result?;
            process_packet(client_data, server_state, raw_packet, was_in_play_state).await?;
            Ok(true)
        }
        () = client_data.keep_alive_tick() => {
            send_keep_alive(client_data).await?;
            Ok(false)
        }
        () = sleep_until_opt(deadline) => {
            Err(PacketProcessingError::Disconnected)
        }
    }
}

/// Sleeps until `deadline`, or never resolves when no deadline is configured
/// (timeout disabled). Used as a `select!` branch to bound connection idleness
/// without arming a timer when the operator has disabled the timeout.
async fn sleep_until_opt(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending::<()>().await,
    }
}

async fn handle_client(socket: TcpStream, server_state: Arc<RwLock<ServerState>>) {
    // Minecraft traffic is many small packets; disabling Nagle's algorithm avoids
    // up to ~40ms of added latency per write and keeps the proxy responsive.
    let _ = socket.set_nodelay(true);

    let client_data = ClientData::new(socket);
    let mut was_in_play_state = false;

    let (login_timeout, read_timeout) = {
        let server_state = server_state.read().await;
        (server_state.login_timeout(), server_state.read_timeout())
    };

    // Idle deadline based on the last *inbound* packet. Connections that send
    // nothing within the timeout are dropped, which is the primary defence against
    // slowloris / half-open connections piling up file descriptors and memory.
    let mut last_activity = Instant::now();

    loop {
        let timeout = if was_in_play_state {
            read_timeout
        } else {
            login_timeout
        };
        let deadline = timeout.map(|timeout| last_activity + timeout);

        match read(&client_data, &server_state, &mut was_in_play_state, deadline).await {
            Ok(received_packet) => {
                // Only inbound packets reset the idle deadline; sending a keep-alive
                // must not keep an otherwise-silent connection alive forever.
                if received_packet {
                    last_activity = Instant::now();
                }
            }
            Err(PacketProcessingError::Disconnected) => {
                debug!("Client disconnected");
                break;
            }
            Err(PacketProcessingError::Custom(e)) => {
                // A single packet failed to handle, but the stream is still framed
                // correctly. Keep the connection and count the received bytes as
                // activity so a legitimate client is not wrongly timed out.
                debug!("Error processing packet: {}", e);
                last_activity = Instant::now();
            }
            Err(PacketProcessingError::DecodePacketError(version, state, packet_id)) => {
                // Unknown packet: ignored per protocol leniency, but bytes were
                // received, so it still counts as activity.
                trace!(
                    "Unknown packet received: version={version} state={state} packet_id={packet_id}"
                );
                last_activity = Instant::now();
            }
        }
    }

    let _ = client_data.shutdown().await;

    if was_in_play_state {
        server_state.write().await.decrement();
        let username = client_data.client().await.get_username();
        info!("{} left the game", username);
    }
}

async fn kick_client(
    client_data: &ClientData,
    reason: String,
) -> Result<(), PacketProcessingError> {
    let (protocol_version, state) = {
        let state = client_data.client().await;
        (state.protocol_version(), state.state())
    };
    let packet = match state {
        State::Login => {
            debug!("Login disconnect");
            PacketRegistry::LoginDisconnect(LoginDisconnectPacket::text(reason))
        }
        State::Configuration => {
            debug!("Configuration disconnect");
            PacketRegistry::ConfigurationDisconnect(DisconnectPacket::text(reason))
        }
        State::Play => {
            debug!("Play disconnect");
            PacketRegistry::PlayDisconnect(DisconnectPacket::text(reason))
        }
        _ => {
            debug!("A user was disconnected from a state where no packet can be sent");
            return Err(PacketProcessingError::Disconnected);
        }
    };
    if let Ok(raw_packet) = packet.encode_packet(protocol_version) {
        client_data.write_packet(raw_packet).await?;
        // Drain the client before closing so the disconnect message is actually
        // delivered instead of the connection being reset (RST) with unread data.
        client_data.disconnect_gracefully().await;
    }

    Ok(())
}

async fn send_keep_alive(client_data: &ClientData) -> Result<(), PacketProcessingError> {
    let (protocol_version, state) = {
        let client = client_data.client().await;
        (client.protocol_version(), client.state())
    };

    if state == State::Play {
        let packet = PacketRegistry::ClientBoundKeepAlive(ClientBoundKeepAlivePacket::random()?);
        let raw_packet = packet.encode_packet(protocol_version)?;
        client_data.write_packet(raw_packet).await?;
    }

    Ok(())
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::custom::CustomOptions;
    use crate::custom::captcha::CaptchaOptions;
    use net::packet_stream::PacketStream;
    use net::raw_packet::RawPacket;
    use tokio::net::TcpStream;

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )]
    fn varint(mut value: i32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            if (value & !0x7f) == 0 {
                out.push(value as u8);
                return out;
            }
            out.push(((value & 0x7f) | 0x80) as u8);
            value = ((value as u32) >> 7) as i32;
        }
    }

    fn mc_string(s: &str) -> Vec<u8> {
        let mut out = varint(i32::try_from(s.len()).unwrap());
        out.extend_from_slice(s.as_bytes());
        out
    }

    async fn write_raw(stream: &mut PacketStream<TcpStream>, id: u8, fields: &[u8]) {
        stream
            .write_packet(RawPacket::from_bytes(id, fields))
            .await
            .unwrap();
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
    }

    /// Full real-socket reproduction of the user's scenario: a 1.21 client with a
    /// Turkish locale, captcha enabled, English kick-message overrides (as in
    /// runMulti.sh). Verifies the Turkish captcha messages arrive intact over the
    /// wire and the connection is never reset before the disconnect.
    #[tokio::test]
    async fn captcha_non_english_flow_over_real_socket() {
        const PROTOCOL_1_21: i32 = 767;

        // Server state mirroring the user's runMulti.sh configuration.
        let mut builder = ServerState::builder();
        builder
            .view_distance(0)
            .welcome_message("")
            .fallback_language("tr".to_string())
            .custom(CustomOptions {
                captcha: CaptchaOptions {
                    enabled: true,
                    failed_kick_message: "Captcha failed. Please try again later.".to_string(),
                    max_attempts: 3,
                    ..CaptchaOptions::default()
                },
                mirror_status: None,
            });
        let server_state = Arc::new(RwLock::new(builder.build().unwrap()));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_state_clone = Arc::clone(&server_state);
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            handle_client(socket, server_state_clone).await;
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let mut cs = PacketStream::new(stream);

        // Handshake (next state = login) + Login Start (name + uuid).
        let mut hs = varint(PROTOCOL_1_21);
        hs.extend(mc_string("127.0.0.1"));
        hs.extend_from_slice(&addr.port().to_be_bytes());
        hs.extend(varint(2));
        write_raw(&mut cs, 0, &hs).await;

        let mut login_start = mc_string("Tester");
        login_start.extend_from_slice(&[0u8; 16]);
        write_raw(&mut cs, 0, &login_start).await;

        // Read Login Success, then acknowledge login.
        let _login_success = cs.read_packet().await.expect("login success");
        write_raw(&mut cs, 3, &[]).await;

        // Configuration: report a Turkish locale (server decodes only the locale).
        write_raw(&mut cs, 0, &mc_string("tr_tr")).await;

        // Respond to Known Packs and acknowledge Finish Configuration.
        loop {
            let packet = cs.read_packet().await.expect("config packet");
            match packet.packet_id() {
                Some(14) => write_raw(&mut cs, 7, &varint(0)).await, // empty known packs
                Some(3) => {
                    write_raw(&mut cs, 3, &[]).await; // ack finish configuration
                    break;
                }
                _ => {}
            }
        }

        // Play: fail the captcha three times to force a deterministic kick.
        for _ in 0..3 {
            write_raw(&mut cs, 6, &mc_string("definitely-wrong")).await;
        }

        // A real client keeps streaming packets (movement, keep-alive responses)
        // after answering. The server stops reading once it decides to kick, so
        // this data sits unread in the socket's receive buffer — on Windows,
        // closing a socket with unread data sends an RST, which makes the client
        // show "Connection reset" instead of the kick message. Reproduce that here.
        for _ in 0..64 {
            write_raw(&mut cs, 6, &mc_string("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")).await;
        }

        // Collect system-chat messages and the final disconnect.
        let mut system_chats: Vec<Vec<u8>> = Vec::new();
        let disconnect;
        loop {
            let packet = cs
                .read_packet()
                .await
                .expect("connection reset before disconnect packet");
            match packet.packet_id() {
                Some(108) => system_chats.push(packet.bytes().to_vec()),
                Some(29) => {
                    disconnect = packet.bytes().to_vec();
                    break;
                }
                _ => {}
            }
        }

        // A Turkish captcha message must have arrived intact: "ı" (0xC4 0xB1) from
        // the copy label or "Ç" (0xC3 0x87) from the solve label.
        let saw_turkish = system_chats
            .iter()
            .any(|c| contains(c, &[0xC4, 0xB1]) || contains(c, &[0xC3, 0x87]));
        assert!(
            saw_turkish,
            "no intact Turkish captcha message received ({} system chats)",
            system_chats.len()
        );

        // The (English-override) kick message must arrive intact on the disconnect.
        assert!(
            contains(&disconnect, b"Captcha failed"),
            "disconnect did not contain the kick message"
        );

        server.await.unwrap();
    }

    /// The user's exact case: a 1.8.9 client (no configuration phase) sends its
    /// Client Settings with a Turkish locale in play state, after the captcha was
    /// already shown in the fallback language. The server must decode that packet
    /// (proving the per-version id) and re-localize the captcha to Turkish.
    #[tokio::test]
    async fn captcha_relocalizes_on_legacy_1_8_locale() {
        const PROTOCOL_1_8: i32 = 47;
        const CLIENT_SETTINGS_1_8: u8 = 0x15;
        const LEGACY_CHAT: u8 = 2; // clientbound legacy_chat_message in 1.8

        let mut builder = ServerState::builder();
        builder
            .view_distance(0)
            .welcome_message("")
            .fallback_language("en".to_string())
            .clear_chat_on_join(true)
            .custom(CustomOptions {
                captcha: CaptchaOptions {
                    enabled: true,
                    ..CaptchaOptions::default()
                },
                mirror_status: None,
            });
        let server_state = Arc::new(RwLock::new(builder.build().unwrap()));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_state_clone = Arc::clone(&server_state);
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            handle_client(socket, server_state_clone).await;
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let mut cs = PacketStream::new(stream);

        // Handshake (next state = login) + 1.8 Login Start (username only).
        let mut hs = varint(PROTOCOL_1_8);
        hs.extend(mc_string("127.0.0.1"));
        hs.extend_from_slice(&addr.port().to_be_bytes());
        hs.extend(varint(2));
        write_raw(&mut cs, 0, &hs).await;
        write_raw(&mut cs, 0, &mc_string("Tester")).await;

        // Read login success (GameProfile); play packets (incl. the fallback
        // captcha) stream in after it.
        let _ = cs.read_packet().await.expect("login success");

        // Now in play: send Client Settings with a Turkish locale. The server only
        // decodes the leading locale string, so no trailing fields are needed.
        write_raw(&mut cs, CLIENT_SETTINGS_1_8, &mc_string("tr_tr")).await;

        // A re-localized Turkish captcha message must arrive: "ı" (0xC4 0xB1) or
        // "Ç" (0xC3 0x87). The earlier English prompt has no such bytes.
        let mut saw_turkish = false;
        for _ in 0..400 {
            let Ok(packet) = cs.read_packet().await else {
                break;
            };
            if packet.packet_id() == Some(LEGACY_CHAT) {
                let bytes = packet.bytes();
                if contains(bytes, &[0xC4, 0xB1]) || contains(bytes, &[0xC3, 0x87]) {
                    saw_turkish = true;
                    break;
                }
            }
        }
        assert!(
            saw_turkish,
            "1.8 client locale did not re-localize the captcha to Turkish"
        );

        drop(cs);
        server.await.unwrap();
    }
}
