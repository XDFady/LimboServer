use serde_json::Value;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};
use tracing::{debug, warn};

#[derive(Clone)]
pub struct MirrorStatus {
    target: String,
    refresh_seconds: u64,
    timeout_seconds: u64,
    cache: Arc<RwLock<Option<MirrorSnapshot>>>,
}

#[derive(Clone)]
pub struct MirrorSnapshot {
    pub motd: String,
    pub online_players: u32,
    pub max_players: u32,
    pub favicon: Option<String>,
}

impl MirrorStatus {
    pub fn new(target: String, refresh_seconds: u64, timeout_seconds: u64) -> Self {
        Self {
            target,
            refresh_seconds: refresh_seconds.max(3),
            timeout_seconds: timeout_seconds.max(1),
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub fn snapshot(&self) -> Option<MirrorSnapshot> {
        self.cache.read().ok()?.clone()
    }

    fn set_snapshot(&self, snapshot: MirrorSnapshot) {
        if let Ok(mut cache) = self.cache.write() {
            *cache = Some(snapshot);
        }
    }
}

pub fn spawn_refresh_task(mirror: MirrorStatus) {
    tokio::spawn(async move {
        loop {
            match ping_java_server(&mirror.target, mirror.timeout_seconds).await {
                Ok(snapshot) => {
                    debug!(
                        "Mirrored {}: {}/{}",
                        mirror.target,
                        snapshot.online_players,
                        snapshot.max_players
                    );

                    mirror.set_snapshot(snapshot);
                }
                Err(err) => {
                    warn!("Failed to mirror {}: {}", mirror.target, err);
                }
            }

            sleep(Duration::from_secs(mirror.refresh_seconds)).await;
        }
    });
}

async fn ping_java_server(
    target: &str,
    timeout_seconds: u64,
) -> Result<MirrorSnapshot, String> {
    let (host, port) = split_host_port(target);
    let address = format!("{host}:{port}");

    let result = timeout(
        Duration::from_secs(timeout_seconds),
        ping_java_server_inner(&address, &host, port),
    )
        .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err("mirror ping timed out".to_string()),
    }
}

async fn ping_java_server_inner(
    address: &str,
    host: &str,
    port: u16,
) -> Result<MirrorSnapshot, String> {
    let mut stream = TcpStream::connect(address)
        .await
        .map_err(|err| format!("connect failed: {err}"))?;

    let handshake = build_handshake_packet(host, port);
    stream
        .write_all(&handshake)
        .await
        .map_err(|err| format!("write handshake failed: {err}"))?;

    let status_request = vec![0x01, 0x00];
    stream
        .write_all(&status_request)
        .await
        .map_err(|err| format!("write status request failed: {err}"))?;

    let _packet_length = read_varint_async(&mut stream).await?;
    let packet_id = read_varint_async(&mut stream).await?;

    if packet_id != 0 {
        return Err(format!("unexpected status packet id: {packet_id}"));
    }

    let json_length = read_varint_async(&mut stream).await? as usize;
    let mut json_bytes = vec![0_u8; json_length];

    stream
        .read_exact(&mut json_bytes)
        .await
        .map_err(|err| format!("read status json failed: {err}"))?;

    let json_text = String::from_utf8(json_bytes)
        .map_err(|err| format!("invalid utf8 in status json: {err}"))?;

    let value: Value = serde_json::from_str(&json_text)
        .map_err(|err| format!("invalid status json: {err}"))?;

    let motd = value
        .get("description")
        .map(description_to_plain_text)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| "Minecraft Server".to_string());

    let online_players = value
        .pointer("/players/online")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(u64::from(u32::MAX)) as u32;

    let max_players = value
        .pointer("/players/max")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(u64::from(u32::MAX)) as u32;

    let favicon = value
        .get("favicon")
        .and_then(Value::as_str)
        .filter(|icon| icon.starts_with("data:image/png;base64,"))
        .map(ToString::to_string);

    Ok(MirrorSnapshot {
        motd,
        online_players,
        max_players,
        favicon,
    })
}

fn split_host_port(target: &str) -> (String, u16) {
    if let Some((host, port_text)) = target.rsplit_once(':') {
        let port = port_text.parse::<u16>().unwrap_or(25565);
        (host.to_string(), port)
    } else {
        (target.to_string(), 25565)
    }
}

fn build_handshake_packet(host: &str, port: u16) -> Vec<u8> {
    let mut payload = Vec::new();

    // Packet ID: Handshake
    write_varint(&mut payload, 0);

    // Protocol version. The exact value does not matter much for status ping.
    // 767 is commonly safe for modern status, but server status generally accepts many values.
    write_varint(&mut payload, 767);

    write_string(&mut payload, host);

    // Port: unsigned short, big endian
    payload.extend_from_slice(&port.to_be_bytes());

    // Next state: 1 = status
    write_varint(&mut payload, 1);

    let mut packet = Vec::new();
    write_varint(&mut packet, payload.len() as i32);
    packet.extend(payload);

    packet
}

fn write_string(buffer: &mut Vec<u8>, value: &str) {
    write_varint(buffer, value.len() as i32);
    buffer.extend_from_slice(value.as_bytes());
}

fn write_varint(buffer: &mut Vec<u8>, mut value: i32) {
    loop {
        if (value & !0x7F) == 0 {
            buffer.push(value as u8);
            return;
        }

        buffer.push(((value & 0x7F) | 0x80) as u8);
        value = ((value as u32) >> 7) as i32;
    }
}

async fn read_varint_async(stream: &mut TcpStream) -> Result<i32, String> {
    let mut num_read = 0;
    let mut result = 0_i32;

    loop {
        let byte = stream
            .read_u8()
            .await
            .map_err(|err| format!("read varint failed: {err}"))?;

        let value = i32::from(byte & 0x7F);
        result |= value << (7 * num_read);

        num_read += 1;

        if num_read > 5 {
            return Err("varint is too big".to_string());
        }

        if (byte & 0x80) == 0 {
            break;
        }
    }

    Ok(result)
}

fn description_to_plain_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),

        Value::Object(map) => {
            let mut output = String::new();

            if let Some(text) = map.get("text").and_then(Value::as_str) {
                output.push_str(text);
            }

            if let Some(extra) = map.get("extra").and_then(Value::as_array) {
                for part in extra {
                    output.push_str(&description_to_plain_text(part));
                }
            }

            output
        }

        Value::Array(items) => items
            .iter()
            .map(description_to_plain_text)
            .collect::<Vec<_>>()
            .join(""),

        _ => String::new(),
    }
}