//! The Unpeel Remote relay protocol, host side — the Rust port of
//! `RelayProtocol.swift`, byte-for-byte. This is what lets a headless host
//! serve a phone OFF the LAN: one outbound WSS to the relay, opaque
//! end-to-end frames inside it. The relay never decrypts anything.
//!
//! Lockstep contract: every derivation label, transcript layout and frame
//! byte here mirrors the Swift file; the conformance test (shipped phone
//! crypto against this implementation, through the real Worker) is the
//! drift guard, exactly like pairing's.

use ring::{aead, agreement, hkdf, hmac, rand::SecureRandom};

pub const VERSION: u32 = 1;
/// Maximum opaque payload accepted by the Relay Worker. The outer
/// `[type][conn id]` data envelope is not part of this budget: the Worker
/// explicitly permits those additional five bytes.
pub const MAX_FRAME_BYTES: usize = 512 * 1024;
/// Every sealed payload carries an eight-byte counter and a 16-byte GCM tag.
pub const AEAD_OVERHEAD_BYTES: usize = 8 + 16;
pub const MAX_SEALED_BYTES: usize = MAX_FRAME_BYTES;
pub const MAX_PLAINTEXT_BYTES: usize = MAX_SEALED_BYTES - AEAD_OVERHEAD_BYTES;
pub const MAX_DEVICE_ID_BYTES: usize = 128;
/// Older Workers admitted the Host's five-byte envelope allowance on client
/// sockets before wrapping the payload. Accept that small excess during a
/// rolling deployment, while new Workers enforce `MAX_SEALED_BYTES`.
pub const LEGACY_CLIENT_PAYLOAD_SLACK_BYTES: usize = 5;
pub const MAX_CLIENT_DATA_FRAME_BYTES: usize =
    6 + MAX_DEVICE_ID_BYTES + MAX_SEALED_BYTES + LEGACY_CLIENT_PAYLOAD_SLACK_BYTES;

pub const fn sealed_frame_fits(byte_count: usize) -> bool {
    byte_count <= MAX_SEALED_BYTES
}

pub const fn plaintext_frame_fits(byte_count: usize) -> bool {
    byte_count <= MAX_PLAINTEXT_BYTES
}

pub const FRAME_HELLO: u8 = 0x01;
pub const FRAME_DATA: u8 = 0x02;
pub const FRAME_CLIENT_CLOSED: u8 = 0x03;
pub const FRAME_CLIENT_DATA: u8 = 0x04;

fn b64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn unb64(text: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

// ─────────────────────────── host frames ───────────────────────────

/// `[0x01][json]` — device registrations (deviceID + relay token hash).
pub fn encode_hello(devices: &[(String, String)]) -> Vec<u8> {
    let list: Vec<serde_json::Value> = devices
        .iter()
        .map(|(device_id, token_hash)| {
            serde_json::json!({ "deviceID": device_id, "tokenHash": token_hash })
        })
        .collect();
    let body = serde_json::json!({ "v": VERSION, "devices": list });
    let mut out = vec![FRAME_HELLO];
    out.extend_from_slice(body.to_string().as_bytes());
    out
}

/// `[0x02][connID u32 BE][opaque]`.
pub fn encode_data(conn_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(FRAME_DATA);
    out.extend_from_slice(&conn_id.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

pub enum Incoming {
    ClientClosed {
        conn_id: u32,
    },
    ClientData {
        conn_id: u32,
        device_id: String,
        payload: Vec<u8>,
    },
}

pub fn decode_incoming(frame: &[u8]) -> Option<Incoming> {
    if frame.len() > MAX_CLIENT_DATA_FRAME_BYTES {
        return None;
    }
    match *frame.first()? {
        FRAME_CLIENT_CLOSED if frame.len() >= 5 => Some(Incoming::ClientClosed {
            conn_id: u32::from_be_bytes(frame[1..5].try_into().ok()?),
        }),
        FRAME_CLIENT_DATA if frame.len() >= 6 => {
            let conn_id = u32::from_be_bytes(frame[1..5].try_into().ok()?);
            let id_len = frame[5] as usize;
            if id_len == 0 || id_len > MAX_DEVICE_ID_BYTES {
                return None;
            }
            let payload_start = 6usize.checked_add(id_len)?;
            if frame.len() < payload_start
                || frame.len() - payload_start
                    > MAX_SEALED_BYTES + LEGACY_CLIENT_PAYLOAD_SLACK_BYTES
            {
                return None;
            }
            let device_id = std::str::from_utf8(&frame[6..payload_start])
                .ok()?
                .to_string();
            Some(Incoming::ClientData {
                conn_id,
                device_id,
                payload: frame[payload_start..].to_vec(),
            })
        }
        _ => None,
    }
}

// ─────────────────────────── handshake ───────────────────────────

pub struct EphemeralKey {
    private: agreement::EphemeralPrivateKey,
    pub public: Vec<u8>,
}

pub fn ephemeral_key() -> Result<EphemeralKey, String> {
    let rng = ring::rand::SystemRandom::new();
    let private = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng)
        .map_err(|_| "x25519 keygen failed".to_string())?;
    let public = private
        .compute_public_key()
        .map_err(|_| "x25519 pubkey failed".to_string())?
        .as_ref()
        .to_vec();
    Ok(EphemeralKey { private, public })
}

pub fn shared_secret(key: EphemeralKey, peer_public: &[u8]) -> Result<Vec<u8>, String> {
    agreement::agree_ephemeral(
        key.private,
        &agreement::UnparsedPublicKey::new(&agreement::X25519, peer_public),
        |secret| secret.to_vec(),
    )
    .map_err(|_| "x25519 agreement failed".to_string())
}

/// HKDF-SHA256 with the protocol's labels. Swift's `HKDF.deriveKey` with no
/// salt parameter uses an all-zero salt of hash length — mirrored here.
fn derive_key(ikm: &[u8], salt: &[u8], info: &str) -> [u8; 32] {
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, salt);
    let prk = salt.extract(ikm);
    let info_parts = [info.as_bytes()];
    let okm = prk
        .expand(&info_parts, hkdf::HKDF_SHA256)
        .expect("hkdf expand");
    let mut out = [0u8; 32];
    okm.fill(&mut out).expect("hkdf fill");
    out
}

/// Keyed by the static device key: proves the host holds it and pins both
/// ephemeral keys against a relay swap. Every field length-prefixed.
pub fn transcript_mac(
    e2e_key: &[u8],
    device_id: &str,
    client_salt: &[u8],
    host_salt: &[u8],
    client_ephemeral: &[u8],
    host_ephemeral: &[u8],
) -> Vec<u8> {
    let mac_key = derive_key(
        e2e_key,
        &[],
        &format!("unpeel-relay-v{VERSION}:handshake-mac"),
    );
    let mut transcript = Vec::new();
    transcript.extend_from_slice(&VERSION.to_be_bytes());
    for field in [
        device_id.as_bytes(),
        client_salt,
        host_salt,
        client_ephemeral,
        host_ephemeral,
    ] {
        transcript.extend_from_slice(&(field.len() as u32).to_be_bytes());
        transcript.extend_from_slice(field);
    }
    let key = hmac::Key::new(hmac::HMAC_SHA256, &mac_key);
    hmac::sign(&key, &transcript).as_ref().to_vec()
}

// ─────────────────────────── crypto session ───────────────────────────

/// Per-connection AEAD channel. Forward secrecy comes from the ephemeral
/// X25519 secret in the IKM; the counter IS the nonce, so a forged counter
/// fails the AEAD open, and strictly-increasing receive order kills replays.
pub struct CryptoSession {
    send_key: aead::LessSafeKey,
    receive_key: aead::LessSafeKey,
    send_tag: [u8; 4],
    receive_tag: [u8; 4],
    send_counter: u64,
    last_received: u64,
}

impl CryptoSession {
    pub fn new(
        e2e_key: &[u8],
        shared: &[u8],
        client_salt: &[u8],
        host_salt: &[u8],
        is_host: bool,
    ) -> Result<Self, String> {
        if e2e_key.len() != 32
            || shared.len() != 32
            || client_salt.len() != 16
            || host_salt.len() != 16
        {
            return Err("handshake failed".into());
        }
        let mut ikm = Vec::with_capacity(64);
        ikm.extend_from_slice(e2e_key);
        ikm.extend_from_slice(shared);
        let mut salt = Vec::with_capacity(32);
        salt.extend_from_slice(client_salt);
        salt.extend_from_slice(host_salt);
        let c2h = derive_key(&ikm, &salt, &format!("unpeel-relay-v{VERSION}:c2h"));
        let h2c = derive_key(&ikm, &salt, &format!("unpeel-relay-v{VERSION}:h2c"));
        let key = |bytes: &[u8]| -> Result<aead::LessSafeKey, String> {
            aead::UnboundKey::new(&aead::AES_256_GCM, bytes)
                .map(aead::LessSafeKey::new)
                .map_err(|_| "bad key".into())
        };
        let (send, recv) = if is_host { (h2c, c2h) } else { (c2h, h2c) };
        Ok(Self {
            send_key: key(&send)?,
            receive_key: key(&recv)?,
            send_tag: if is_host { *b"h2c!" } else { *b"c2h!" },
            receive_tag: if is_host { *b"c2h!" } else { *b"h2c!" },
            send_counter: 0,
            last_received: 0,
        })
    }

    fn nonce(tag: &[u8; 4], counter: u64) -> aead::Nonce {
        let mut bytes = [0u8; 12];
        bytes[..4].copy_from_slice(tag);
        bytes[4..].copy_from_slice(&counter.to_be_bytes());
        aead::Nonce::assume_unique_for_key(bytes)
    }

    /// `[counter u64 BE][ciphertext‖tag]`.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        if !plaintext_frame_fits(plaintext.len()) {
            return Err("relay plaintext frame too large".into());
        }
        self.send_counter = self
            .send_counter
            .checked_add(1)
            .ok_or("counter exhausted")?;
        let mut body = plaintext.to_vec();
        self.send_key
            .seal_in_place_append_tag(
                Self::nonce(&self.send_tag, self.send_counter),
                aead::Aad::empty(),
                &mut body,
            )
            .map_err(|_| "seal failed".to_string())?;
        let mut out = self.send_counter.to_be_bytes().to_vec();
        out.extend_from_slice(&body);
        debug_assert!(sealed_frame_fits(out.len()));
        Ok(out)
    }

    pub fn open(&mut self, frame: &[u8]) -> Result<Vec<u8>, String> {
        if frame.len() < 8 + 16 {
            return Err("open failed".into());
        }
        let counter = u64::from_be_bytes(frame[..8].try_into().unwrap());
        if counter <= self.last_received {
            return Err("replay detected".into());
        }
        let mut body = frame[8..].to_vec();
        let plaintext = self
            .receive_key
            .open_in_place(
                Self::nonce(&self.receive_tag, counter),
                aead::Aad::empty(),
                &mut body,
            )
            .map_err(|_| "open failed".to_string())?
            .to_vec();
        self.last_received = counter;
        Ok(plaintext)
    }
}

// ─────────────────────── handshake messages + tunnel ───────────────────────

pub struct ClientHello {
    pub device_id: String,
    pub salt: Vec<u8>,
    pub ephemeral_public: Vec<u8>,
}

pub fn parse_client_hello(payload: &[u8]) -> Option<ClientHello> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    if value.get("v")?.as_u64()? != VERSION as u64 {
        return None;
    }
    let salt = unb64(value.get("saltB64")?.as_str()?)?;
    let ephemeral = unb64(value.get("ephemeralPublicKeyB64")?.as_str()?)?;
    if salt.len() != 16 {
        return None;
    }
    Some(ClientHello {
        device_id: value.get("deviceID")?.as_str()?.to_string(),
        salt,
        ephemeral_public: ephemeral,
    })
}

pub fn encode_host_hello(salt: &[u8], ephemeral_public: &[u8], mac: &[u8]) -> Vec<u8> {
    serde_json::json!({
        "v": VERSION,
        "saltB64": b64(salt),
        "ephemeralPublicKeyB64": b64(ephemeral_public),
        "macB64": b64(mac),
    })
    .to_string()
    .into_bytes()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelRequest {
    pub id: u64,
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub auth: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Encode the transport-neutral request envelope used inside both Relay AEAD
/// and SSH stdio framing. Authentication is optional because paired/Relay
/// transports carry a device credential while SSH derives owner authority
/// from the remote Unix account and ignores this field.
pub fn encode_tunnel_request(request: &TunnelRequest) -> Vec<u8> {
    let query: serde_json::Map<String, serde_json::Value> = request
        .query
        .iter()
        .map(|(key, value)| (key.clone(), value.clone().into()))
        .collect();
    serde_json::json!({
        "id": request.id,
        "method": request.method,
        "path": request.path,
        "query": query,
        "auth": request.auth,
        "contentType": request.content_type,
        "bodyB64": if request.body.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(b64(&request.body))
        },
    })
    .to_string()
    .into_bytes()
}

/// Strict request decoding for owner transports. The legacy Relay helper
/// below remains an `Option`, but SSH needs a useful protocol error and must
/// never turn malformed base64 into an empty mutation body.
pub fn parse_tunnel_request_strict(plaintext: &[u8]) -> Result<TunnelRequest, String> {
    let value: serde_json::Value =
        serde_json::from_slice(plaintext).map_err(|_| "request is not valid JSON")?;
    let object = value
        .as_object()
        .ok_or("request envelope must be an object")?;
    let query = match object.get("query") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::Object(query)) => query
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|value| (key.clone(), value.to_owned()))
                    .ok_or_else(|| "query values must be strings".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("query must be an object".into()),
    };
    let optional_string = |key: &str| -> Result<Option<String>, String> {
        match object.get(key) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(format!("{key} must be a string")),
        }
    };
    let body = match optional_string("bodyB64")? {
        Some(value) => unb64(&value).ok_or("bodyB64 is not valid base64")?,
        None => Vec::new(),
    };
    Ok(TunnelRequest {
        id: object
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .ok_or("id must be an unsigned integer")?,
        method: object
            .get("method")
            .and_then(serde_json::Value::as_str)
            .ok_or("method must be a string")?
            .to_owned(),
        path: object
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or("path must be a string")?
            .to_owned(),
        query,
        auth: optional_string("auth")?,
        content_type: optional_string("contentType")?,
        body,
    })
}

pub fn parse_tunnel_request(plaintext: &[u8]) -> Option<TunnelRequest> {
    let value: serde_json::Value = serde_json::from_slice(plaintext).ok()?;
    let query = value
        .get("query")
        .and_then(|q| q.as_object())
        .map(|object| {
            object
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Some(TunnelRequest {
        id: value.get("id")?.as_u64()?,
        method: value.get("method")?.as_str()?.to_string(),
        path: value.get("path")?.as_str()?.to_string(),
        query,
        auth: value
            .get("auth")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        content_type: value
            .get("contentType")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        body: value
            .get("bodyB64")
            .and_then(|v| v.as_str())
            .and_then(unb64)
            .unwrap_or_default(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelResponse {
    pub id: u64,
    pub status: u16,
    pub body: Vec<u8>,
}

pub fn parse_tunnel_response(plaintext: &[u8]) -> Result<TunnelResponse, String> {
    let value: serde_json::Value =
        serde_json::from_slice(plaintext).map_err(|_| "response is not valid JSON")?;
    let object = value
        .as_object()
        .ok_or("response envelope must be an object")?;
    let body = match object.get("bodyB64") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::String(value)) => {
            unb64(value).ok_or("bodyB64 is not valid base64")?
        }
        Some(_) => return Err("bodyB64 must be a string".into()),
    };
    let status = object
        .get("status")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or("status must be an unsigned 16-bit integer")?;
    Ok(TunnelResponse {
        id: object
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .ok_or("id must be an unsigned integer")?,
        status,
        body,
    })
}

pub fn encode_tunnel_response(id: u64, status: u16, body: &[u8]) -> Vec<u8> {
    serde_json::json!({
        "id": id,
        "status": status,
        "bodyB64": if body.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(b64(body))
        },
    })
    .to_string()
    .into_bytes()
}

/// Encode a tunneled response without ever handing an oversized plaintext to
/// the crypto/session/socket pipeline. A large route response becomes a small
/// correlated 413, keeping the host uplink alive for this and other phones.
pub fn encode_bounded_tunnel_response(id: u64, status: u16, body: &[u8]) -> Vec<u8> {
    let response = encode_tunnel_response(id, status, body);
    if plaintext_frame_fits(response.len()) {
        return response;
    }

    let replacement = encode_tunnel_response(id, 413, br#"{"error":"response too large"}"#);
    assert!(
        plaintext_frame_fits(replacement.len()),
        "413 relay response must fit the plaintext frame budget"
    );
    replacement
}

pub fn random_bytes(count: usize) -> Vec<u8> {
    let mut out = vec![0u8; count];
    let _ = ring::rand::SystemRandom::new().fill(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full handshake, both roles played by this implementation: a
    /// structural proof the derivations agree with themselves. Cross-impl
    /// agreement with the shipped Swift is the conformance test's job.
    #[test]
    fn handshake_and_session_round_trip() {
        let e2e = random_bytes(32);
        let client = ephemeral_key().unwrap();
        let host = ephemeral_key().unwrap();
        let client_pub = client.public.clone();
        let host_pub = host.public.clone();
        let client_secret = shared_secret(client, &host_pub).unwrap();
        let host_secret = shared_secret(host, &client_pub).unwrap();
        assert_eq!(client_secret, host_secret, "ECDH must agree");

        let client_salt = random_bytes(16);
        let host_salt = random_bytes(16);
        let mut host_session =
            CryptoSession::new(&e2e, &host_secret, &client_salt, &host_salt, true).unwrap();
        let mut phone_session =
            CryptoSession::new(&e2e, &client_secret, &client_salt, &host_salt, false).unwrap();

        let sealed = host_session.seal(b"hello phone").unwrap();
        assert_eq!(phone_session.open(&sealed).unwrap(), b"hello phone");
        let back = phone_session.seal(b"hello host").unwrap();
        assert_eq!(host_session.open(&back).unwrap(), b"hello host");
        // Replay must die.
        assert!(host_session.open(&back).is_err());
    }

    #[test]
    fn relay_frame_boundaries_include_the_exact_aead_overhead() {
        assert_eq!(AEAD_OVERHEAD_BYTES, 24);
        assert_eq!(MAX_PLAINTEXT_BYTES + AEAD_OVERHEAD_BYTES, MAX_SEALED_BYTES);
        assert!(plaintext_frame_fits(MAX_PLAINTEXT_BYTES));
        assert!(!plaintext_frame_fits(MAX_PLAINTEXT_BYTES + 1));
        assert!(sealed_frame_fits(MAX_SEALED_BYTES));
        assert!(!sealed_frame_fits(MAX_SEALED_BYTES + 1));

        let e2e = [7u8; 32];
        let shared = [8u8; 32];
        let client_salt = [9u8; 16];
        let host_salt = [10u8; 16];
        let mut session =
            CryptoSession::new(&e2e, &shared, &client_salt, &host_salt, true).unwrap();
        let sealed = session.seal(&vec![0; MAX_PLAINTEXT_BYTES]).unwrap();
        assert_eq!(sealed.len(), MAX_SEALED_BYTES);
        assert_eq!(session.send_counter, 1);
        assert!(session.seal(&vec![0; MAX_PLAINTEXT_BYTES + 1]).is_err());
        assert_eq!(
            session.send_counter, 1,
            "a rejected frame cannot burn a nonce"
        );
    }

    #[test]
    fn mac_binds_every_field() {
        let e2e = vec![7u8; 32];
        let base = transcript_mac(&e2e, "dev-1", &[1; 16], &[2; 16], &[3; 32], &[4; 32]);
        assert_eq!(base.len(), 32);
        // Any field change changes the MAC — including the length-shift
        // attack the prefixes exist for.
        let shifted = transcript_mac(&e2e, "dev-", &[0x31; 16], &[2; 16], &[3; 32], &[4; 32]);
        assert_ne!(base, shifted);
        assert_ne!(
            base,
            transcript_mac(&e2e, "dev-1", &[1; 16], &[2; 16], &[3; 32], &[5; 32])
        );
    }

    #[test]
    fn frames_round_trip_and_reject_garbage() {
        let hello = encode_hello(&[("dev-1".into(), "ab".repeat(32))]);
        assert_eq!(hello[0], FRAME_HELLO);
        let data = encode_data(7, b"payload");
        assert_eq!(&data[..5], &[FRAME_DATA, 0, 0, 0, 7]);

        let client = {
            let mut f = vec![FRAME_CLIENT_DATA, 0, 0, 0, 9, 5];
            f.extend_from_slice(b"dev-1");
            f.extend_from_slice(b"opaque");
            f
        };
        match decode_incoming(&client) {
            Some(Incoming::ClientData {
                conn_id,
                device_id,
                payload,
            }) => {
                assert_eq!(
                    (conn_id, device_id.as_str(), payload.as_slice()),
                    (9, "dev-1", b"opaque".as_slice())
                );
            }
            _ => panic!("client data frame must decode"),
        }
        assert!(decode_incoming(&[0xff, 0, 0]).is_none());
        assert!(matches!(
            decode_incoming(&[FRAME_CLIENT_CLOSED, 0, 0, 0, 3]),
            Some(Incoming::ClientClosed { conn_id: 3 })
        ));

        let device_id = "d".repeat(MAX_DEVICE_ID_BYTES);
        let mut legacy_boundary = vec![FRAME_CLIENT_DATA, 0, 0, 0, 11];
        legacy_boundary.push(MAX_DEVICE_ID_BYTES as u8);
        legacy_boundary.extend_from_slice(device_id.as_bytes());
        legacy_boundary.resize(MAX_CLIENT_DATA_FRAME_BYTES, 0x5a);
        assert_eq!(legacy_boundary.len(), MAX_CLIENT_DATA_FRAME_BYTES);
        assert!(matches!(
            decode_incoming(&legacy_boundary),
            Some(Incoming::ClientData { conn_id: 11, payload, .. })
                if payload.len() == MAX_SEALED_BYTES + LEGACY_CLIENT_PAYLOAD_SLACK_BYTES
        ));

        legacy_boundary.push(0);
        assert!(decode_incoming(&legacy_boundary).is_none());

        let mut oversized_short_id = vec![FRAME_CLIENT_DATA, 0, 0, 0, 12, 1, b'd'];
        oversized_short_id.resize(
            7 + MAX_SEALED_BYTES + LEGACY_CLIENT_PAYLOAD_SLACK_BYTES + 1,
            0x6b,
        );
        assert!(decode_incoming(&oversized_short_id).is_none());
    }

    #[test]
    fn tunnel_shapes_match_the_swift_dialect() {
        let request = parse_tunnel_request(
            br#"{"id":4,"method":"GET","path":"/mobile/bootstrap","query":{"a":"1"},"auth":"tok","bodyB64":null}"#,
        )
        .unwrap();
        assert_eq!(request.id, 4);
        assert_eq!(request.path, "/mobile/bootstrap");
        assert_eq!(request.auth.as_deref(), Some("tok"));
        assert_eq!(request.content_type, None);
        let response = encode_tunnel_response(4, 200, b"{}");
        let value: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(value["status"], 200);
        assert_eq!(value["bodyB64"], "e30=");
        assert_eq!(
            parse_tunnel_response(&response).unwrap(),
            TunnelResponse {
                id: 4,
                status: 200,
                body: b"{}".to_vec(),
            }
        );
    }

    #[test]
    fn tunnel_request_encoder_round_trips_binary_and_strictly_rejects_bad_base64() {
        let request = TunnelRequest {
            id: 19,
            method: "POST".into(),
            path: "/mobile/upload-chunk".into(),
            query: vec![("name".into(), "hei 🙂".into())],
            auth: Some("ignored by SSH".into()),
            content_type: Some("image/png".into()),
            body: vec![0, 1, 2, 0xff],
        };
        let decoded = parse_tunnel_request_strict(&encode_tunnel_request(&request)).unwrap();
        assert_eq!(decoded.id, request.id);
        assert_eq!(decoded.method, request.method);
        assert_eq!(decoded.path, request.path);
        assert_eq!(decoded.query, request.query);
        assert_eq!(decoded.auth, request.auth);
        assert_eq!(decoded.content_type, request.content_type);
        assert_eq!(decoded.body, request.body);

        assert!(parse_tunnel_request_strict(
            br#"{"id":19,"method":"POST","path":"/mobile/write","bodyB64":"%%%"}"#,
        )
        .is_err());
        assert!(parse_tunnel_request_strict(
            br#"{"id":19,"method":"GET","path":"/mobile/bootstrap","query":{"bad":2}}"#,
        )
        .is_err());
    }

    #[test]
    fn tunnel_request_preserves_shipped_swift_fields() {
        let request = parse_tunnel_request(
            br#"{"id":42,"method":"POST","path":"/mobile/image","query":{"session_id":"s1","kind":"attachment"},"auth":"Bearer full-token","contentType":"image/png","bodyB64":"iVBORw0KGgo="}"#,
        )
        .unwrap();

        assert_eq!(request.id, 42);
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/mobile/image");
        assert!(request.query.contains(&("session_id".into(), "s1".into())));
        assert!(request
            .query
            .contains(&("kind".into(), "attachment".into())));
        assert_eq!(request.auth.as_deref(), Some("Bearer full-token"));
        assert_eq!(request.content_type.as_deref(), Some("image/png"));
        assert_eq!(
            request.body,
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }

    #[test]
    fn bounded_tunnel_response_keeps_the_largest_encoding_and_replaces_the_next() {
        let fits = |body_len| {
            plaintext_frame_fits(encode_tunnel_response(91, 200, &vec![b'x'; body_len]).len())
        };
        let mut low = 0;
        let mut high = MAX_PLAINTEXT_BYTES;
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if fits(middle) {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let largest_body = low;
        assert!(fits(largest_body));
        assert!(!fits(largest_body + 1));

        let boundary = encode_bounded_tunnel_response(91, 200, &vec![b'x'; largest_body]);
        assert_eq!(
            boundary,
            encode_tunnel_response(91, 200, &vec![b'x'; largest_body])
        );

        let replacement = encode_bounded_tunnel_response(91, 200, &vec![b'x'; largest_body + 1]);
        assert!(plaintext_frame_fits(replacement.len()));
        let value: serde_json::Value = serde_json::from_slice(&replacement).unwrap();
        assert_eq!(value["id"], 91);
        assert_eq!(value["status"], 413);
        assert_eq!(
            unb64(value["bodyB64"].as_str().unwrap()).unwrap(),
            br#"{"error":"response too large"}"#
        );
    }
}
