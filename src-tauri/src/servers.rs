use crate::error::{AppError, Result};
use crate::models::{ServerEntry, ServerStatus};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tauri::command]
pub fn list_servers(state: tauri::State<'_, crate::AppState>) -> Result<Vec<ServerEntry>> {
    state.store.servers()
}

#[tauri::command]
pub fn upsert_server(
    mut server: ServerEntry,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ServerEntry> {
    if server.name.trim().is_empty() || server.address.trim().is_empty() {
        return Err(AppError::new("Nom et adresse du serveur requis"));
    }
    if server.id.is_empty() {
        server.id = uuid::Uuid::new_v4().to_string();
    }
    server.name = server.name.trim().into();
    server.address = server.address.trim().into();
    let mut servers = state.store.servers()?;
    if let Some(existing) = servers.iter_mut().find(|item| item.id == server.id) {
        *existing = server.clone();
    } else {
        servers.push(server.clone());
    }
    state.store.save_servers(&servers)?;
    Ok(server)
}

#[tauri::command]
pub fn delete_server(
    id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ServerEntry>> {
    let mut servers = state.store.servers()?;
    servers.retain(|server| server.id != id);
    state.store.save_servers(&servers)?;
    Ok(servers)
}

fn write_varint(output: &mut Vec<u8>, mut value: i32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

async fn read_varint(stream: &mut TcpStream) -> Result<i32> {
    let mut value = 0_i32;
    for position in 0..5 {
        let byte = stream.read_u8().await?;
        value |= ((byte & 0x7f) as i32) << (7 * position);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(AppError::new("VarInt invalide dans la réponse du serveur"))
}

fn packet(id: i32, body: &[u8]) -> Vec<u8> {
    let mut content = Vec::new();
    write_varint(&mut content, id);
    content.extend(body);
    let mut result = Vec::new();
    write_varint(&mut result, content.len() as i32);
    result.extend(content);
    result
}

fn chat_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    let mut text = value["text"].as_str().unwrap_or("").to_string();
    if let Some(extra) = value["extra"].as_array() {
        for part in extra {
            text.push_str(&chat_text(part));
        }
    }
    text
}

#[tauri::command]
pub async fn query_server(server: ServerEntry) -> Result<ServerStatus> {
    let result = tokio::time::timeout(Duration::from_secs(6), status_request(&server)).await;
    match result {
        Ok(Ok((json, latency))) => Ok(ServerStatus {
            online: true,
            latency: Some(latency),
            online_players: json["players"]["online"].as_u64(),
            max_players: json["players"]["max"].as_u64(),
            motd: json.get("description").map(chat_text),
            version: json["version"]["name"].as_str().map(str::to_string),
            icon: json["favicon"].as_str().map(str::to_string),
            error: None,
            server,
        }),
        Ok(Err(error)) => Ok(ServerStatus {
            online: false,
            latency: None,
            online_players: None,
            max_players: None,
            motd: None,
            version: None,
            icon: None,
            error: Some(error.to_string()),
            server,
        }),
        Err(_) => Ok(ServerStatus {
            online: false,
            latency: None,
            online_players: None,
            max_players: None,
            motd: None,
            version: None,
            icon: None,
            error: Some("Le serveur ne répond pas dans le délai imparti".into()),
            server,
        }),
    }
}

async fn status_request(server: &ServerEntry) -> Result<(Value, u64)> {
    let started = Instant::now();
    let mut stream = TcpStream::connect((server.address.as_str(), server.port)).await?;
    let mut handshake = Vec::new();
    write_varint(&mut handshake, 47);
    write_varint(&mut handshake, server.address.len() as i32);
    handshake.extend(server.address.as_bytes());
    handshake.extend(server.port.to_be_bytes());
    write_varint(&mut handshake, 1);
    stream.write_all(&packet(0, &handshake)).await?;
    stream.write_all(&packet(0, &[])).await?;
    let length = read_varint(&mut stream).await?;
    if length <= 0 || length > 1_048_576 {
        return Err(AppError::new("Réponse du serveur trop volumineuse"));
    }
    let mut buffer = vec![0_u8; length as usize];
    stream.read_exact(&mut buffer).await?;
    let mut cursor = std::io::Cursor::new(buffer);
    let packet_id = read_cursor_varint(&mut cursor)?;
    if packet_id != 0 {
        return Err(AppError::new("Réponse de statut inattendue"));
    }
    let json_length = read_cursor_varint(&mut cursor)? as usize;
    let position = cursor.position() as usize;
    let bytes = cursor.into_inner();
    if position + json_length > bytes.len() {
        return Err(AppError::new("Réponse JSON tronquée"));
    }
    let json = serde_json::from_slice(&bytes[position..position + json_length])?;
    Ok((json, started.elapsed().as_millis() as u64))
}

fn read_cursor_varint(cursor: &mut std::io::Cursor<Vec<u8>>) -> Result<i32> {
    let mut value = 0_i32;
    for position in 0..5 {
        let mut byte = [0_u8; 1];
        std::io::Read::read_exact(cursor, &mut byte)?;
        value |= ((byte[0] & 0x7f) as i32) << (7 * position);
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(AppError::new("VarInt invalide"))
}

#[cfg(test)]
mod tests {
    use super::{packet, read_cursor_varint};

    #[test]
    fn builds_and_reads_protocol_packets() {
        let packet = packet(0, &[1, 2, 3]);
        assert_eq!(packet, vec![4, 0, 1, 2, 3]);
        let mut cursor = std::io::Cursor::new(vec![0x80, 0x01]);
        assert_eq!(read_cursor_varint(&mut cursor).unwrap(), 128);
    }
}
