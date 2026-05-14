use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::state::AppState;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatusDto {
    pub online: bool,
    pub ping_ms: Option<u64>,
    pub players_online: Option<u32>,
    pub players_max: Option<u32>,
    pub motd: Option<String>,
    pub version: Option<String>,
}

// ── Server List Ping (SLP) — Protocolo moderno Minecraft 1.7+ ────────────────

fn write_varint(buf: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            buf.push(byte | 0x80);
        } else {
            buf.push(byte);
            break;
        }
    }
}

async fn read_varint(stream: &mut TcpStream) -> std::io::Result<i32> {
    let mut result = 0i32;
    let mut shift = 0u32;
    loop {
        let byte = stream.read_u8().await?;
        result |= ((byte & 0x7F) as i32) << shift;
        if byte & 0x80 == 0 { break; }
        shift += 7;
        if shift >= 35 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "VarInt demasiado largo",
            ));
        }
    }
    Ok(result)
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_varint(buf, bytes.len() as i32);
    buf.extend_from_slice(bytes);
}

async fn read_string(stream: &mut TcpStream) -> std::io::Result<String> {
    let len = read_varint(stream).await? as usize;
    let mut bytes = vec![0u8; len];
    stream.read_exact(&mut bytes).await?;
    String::from_utf8(bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Construye el paquete Handshake (0x00) del protocolo SLP.
fn handshake_packet(address: &str, port: u16) -> Vec<u8> {
    let mut payload = Vec::new();
    write_varint(&mut payload, 0x00);        // Packet ID
    write_varint(&mut payload, 765);         // Protocol version (1.20.4, suficientemente moderno)
    write_string(&mut payload, address);
    payload.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut payload, 1);           // Next state: status

    let mut packet = Vec::new();
    write_varint(&mut packet, payload.len() as i32);
    packet.extend(payload);
    packet
}

/// Construye el paquete Status Request (0x00, sin payload).
fn status_request_packet() -> Vec<u8> {
    vec![0x01, 0x00] // length=1, packet_id=0x00
}

async fn do_ping(address: &str, port: u16) -> std::io::Result<ServerStatusDto> {
    let start = Instant::now();
    let mut stream = TcpStream::connect((address, port)).await?;

    // Handshake + Status Request
    stream.write_all(&handshake_packet(address, port)).await?;
    stream.write_all(&status_request_packet()).await?;

    // Leer respuesta: VarInt(length) + VarInt(packet_id) + String(json)
    let _length    = read_varint(&mut stream).await?;
    let packet_id  = read_varint(&mut stream).await?;
    if packet_id != 0x00 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Se esperaba packet_id 0x00, llegó 0x{packet_id:02X}"),
        ));
    }
    let json_str = read_string(&mut stream).await?;
    let ping_ms  = start.elapsed().as_millis() as u64;

    // Parsear JSON de respuesta SLP
    let v: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let players_online = v["players"]["online"].as_u64().map(|n| n as u32);
    let players_max    = v["players"]["max"].as_u64().map(|n| n as u32);
    // MOTD puede ser {text: "..."} o directamente un string
    let motd = v["description"]["text"]
        .as_str()
        .or_else(|| v["description"].as_str())
        .map(|s| s.to_string());
    let version = v["version"]["name"].as_str().map(|s| s.to_string());

    Ok(ServerStatusDto {
        online: true,
        ping_ms: Some(ping_ms),
        players_online,
        players_max,
        motd,
        version,
    })
}

/// Consulta el estado del servidor usando el protocolo Server List Ping (SLP).
/// Devuelve { online: false } si hay error o timeout en vez de propagar el error.
#[tauri::command]
pub async fn server_status(state: State<'_, AppState>) -> Result<ServerStatusDto, String> {
    let offline = || ServerStatusDto {
        online: false,
        ping_ms: None,
        players_online: None,
        players_max: None,
        motd: None,
        version: None,
    };

    // Resolver dirección de la instancia activa
    let (address, port) = {
        let instance_id = state.active_instance.lock().await.clone();
        let remote = state.remote_instances.lock().await;
        let source: Vec<_> = match remote.as_ref() {
            Some(list) if !list.is_empty() => list.clone(),
            _ => state.config.effective_instances(),
        };
        let inst = source.into_iter().find(|i| i.id == instance_id);
        match inst {
            Some(i) if !i.server_address.is_empty() => (i.server_address, i.server_port),
            _ if !state.config.server.address.is_empty() =>
                (state.config.server.address.clone(), state.config.server.port),
            _ => return Ok(offline()),
        }
    };

    match tokio::time::timeout(Duration::from_secs(5), do_ping(&address, port)).await {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(e)) => {
            tracing::debug!("server_status ping failed ({address}:{port}): {e}");
            Ok(offline())
        }
        Err(_) => {
            tracing::debug!("server_status timeout ({address}:{port})");
            Ok(offline())
        }
    }
}
