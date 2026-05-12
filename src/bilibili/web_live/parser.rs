pub const OP_HEARTBEAT: u32 = 2;
pub const OP_HEARTBEAT_REPLY: u32 = 3;
pub const OP_MESSAGE: u32 = 5;
pub const OP_AUTH: u32 = 7;
pub const OP_CONNECT_SUCCESS: u32 = 8;

pub const HEADER_LEN: usize = 16;
pub const PROTOVER_PLAIN: u16 = 0;
pub const PROTOVER_ZLIB: u16 = 2;
pub const PROTOVER_BROTLI: u16 = 3;

pub fn build_packet(op: u32, body: &str) -> Vec<u8> {
    let body_bytes = body.as_bytes();
    let total_len = HEADER_LEN + body_bytes.len();
    let mut buf = vec![0u8; total_len];
    buf[0..4].copy_from_slice(&(total_len as u32).to_be_bytes());
    buf[4..6].copy_from_slice(&(HEADER_LEN as u16).to_be_bytes());
    buf[6..8].copy_from_slice(&PROTOVER_PLAIN.to_be_bytes());
    buf[8..12].copy_from_slice(&op.to_be_bytes());
    buf[12..16].copy_from_slice(&1u32.to_be_bytes());
    buf[HEADER_LEN..].copy_from_slice(body_bytes);
    buf
}

#[derive(Debug, Clone)]
pub struct ParsedPacket {
    pub protover: u16,
    pub op: u32,
    pub body: Vec<u8>,
}

pub fn parse_packets(buf: &[u8]) -> Vec<ParsedPacket> {
    let mut packets = Vec::new();
    let mut offset = 0;
    while offset + HEADER_LEN <= buf.len() {
        let total_len = u32::from_be_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;
        let header_len =
            u16::from_be_bytes(buf[offset + 4..offset + 6].try_into().unwrap()) as usize;
        let protover = u16::from_be_bytes(buf[offset + 6..offset + 8].try_into().unwrap());
        let op = u32::from_be_bytes(buf[offset + 8..offset + 12].try_into().unwrap());
        if total_len < header_len || total_len > buf.len() - offset || header_len != HEADER_LEN {
            break;
        }
        let body = buf[offset + header_len..offset + total_len].to_vec();
        packets.push(ParsedPacket { protover, op, body });
        offset += total_len;
    }
    packets
}

pub fn decompress_body(protover: u16, body: &[u8]) -> Result<Vec<u8>, String> {
    match protover {
        PROTOVER_ZLIB => {
            use std::io::Read;
            let mut decoder = flate2::read::ZlibDecoder::new(body);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(|e| format!("zlib error: {e}"))?;
            Ok(out)
        }
        PROTOVER_BROTLI => {
            use std::io::Read;
            let mut decoder = brotli::Decompressor::new(body, 4096);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(|e| format!("brotli error: {e}"))?;
            Ok(out)
        }
        _ => Err(format!("unknown protover for decompression: {protover}")),
    }
}

pub fn extract_json_messages(body: &[u8]) -> Vec<serde_json::Value> {
    let text = String::from_utf8_lossy(body);
    let mut messages = Vec::new();
    for chunk in text.split(|c: char| c.is_control()) {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(start) = trimmed.find('{') else {
            continue;
        };
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&trimmed[start..]) {
            if parsed.is_object() {
                messages.push(parsed);
            }
        }
    }
    messages
}

pub fn collect_commands(protover: u16, body: &[u8]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    match protover {
        PROTOVER_PLAIN | 1 => {
            out.extend(extract_json_messages(body));
        }
        PROTOVER_ZLIB | PROTOVER_BROTLI => match decompress_body(protover, body) {
            Ok(decompressed) => {
                for packet in parse_packets(&decompressed) {
                    if packet.op == OP_MESSAGE {
                        out.extend(collect_commands(packet.protover, &packet.body));
                    }
                }
            }
            Err(e) => {
                tracing::error!("decompression error: {e}");
            }
        },
        _ => {
            tracing::warn!("unknown protover: {protover}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_auth_packet_roundtrip() {
        let body = r#"{"key":"abc","roomid":123}"#;
        let packet = build_packet(OP_AUTH, body);
        assert!(packet.len() > HEADER_LEN);
        let parsed = parse_packets(&packet);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].op, OP_AUTH);
        assert_eq!(String::from_utf8(parsed[0].body.clone()).unwrap(), body);
    }

    #[test]
    fn test_build_heartbeat_packet() {
        let packet = build_packet(OP_HEARTBEAT, "");
        assert_eq!(packet.len(), HEADER_LEN);
        let parsed = parse_packets(&packet);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].op, OP_HEARTBEAT);
        assert!(parsed[0].body.is_empty());
    }

    #[test]
    fn test_parse_multiple_packets() {
        let p1 = build_packet(OP_CONNECT_SUCCESS, "");
        let p2 = build_packet(OP_MESSAGE, r#"{"cmd":"test"}"#);
        let mut combined = p1;
        combined.extend_from_slice(&p2);
        let parsed = parse_packets(&combined);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].op, OP_CONNECT_SUCCESS);
        assert_eq!(parsed[1].op, OP_MESSAGE);
    }

    #[test]
    fn test_parse_rejects_truncated_header() {
        let data = vec![0u8; 8];
        assert!(parse_packets(&data).is_empty());
    }

    #[test]
    fn test_parse_rejects_inconsistent_lengths() {
        let mut packet = vec![0u8; HEADER_LEN];
        packet[0..4].copy_from_slice(&8u32.to_be_bytes());
        packet[4..6].copy_from_slice(&(HEADER_LEN as u16).to_be_bytes());
        assert!(parse_packets(&packet).is_empty());
    }

    #[test]
    fn test_decompress_zlib() {
        let original = b"hello world";
        let mut compressed = Vec::new();
        {
            use std::io::Write;
            let mut encoder =
                flate2::write::ZlibEncoder::new(&mut compressed, flate2::Compression::default());
            encoder.write_all(original).unwrap();
        }
        let result = decompress_body(PROTOVER_ZLIB, &compressed).unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn test_decompress_unknown_protover() {
        assert!(decompress_body(99, b"test").is_err());
    }

    #[test]
    fn test_extract_single_json() {
        let body = br#"{"cmd":"SEND_GIFT","data":{}}"#;
        let msgs = extract_json_messages(body);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["cmd"], "SEND_GIFT");
    }

    #[test]
    fn test_extract_multiple_with_control_chars() {
        let body = b"{\"cmd\":\"A\"}\x00\x01{\"cmd\":\"B\"}";
        let msgs = extract_json_messages(body);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["cmd"], "A");
        assert_eq!(msgs[1]["cmd"], "B");
    }

    #[test]
    fn test_extract_skips_non_json_prefix() {
        let body = b"some garbage {\"cmd\":\"OK\"}";
        let msgs = extract_json_messages(body);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["cmd"], "OK");
    }

    #[test]
    fn test_extract_empty() {
        assert!(extract_json_messages(b"").is_empty());
    }

    #[test]
    fn test_collect_commands_plain() {
        let body = r#"{"cmd":"DANMU_MSG"}"#;
        let inner = build_packet(OP_MESSAGE, body);
        let outer = build_packet(OP_MESSAGE, &String::from_utf8_lossy(&inner[HEADER_LEN..]));
        let cmds = collect_commands(1, &outer[HEADER_LEN..]);
        assert!(!cmds.is_empty());
    }
}
