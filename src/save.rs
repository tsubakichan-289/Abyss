use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};

use crate::game::{Game, GameSnapshot};

const SAVE_VERSION: u32 = 1;
const SAVE_SECRET: &[u8] = b"abyss-save-secret-v1-rotate-if-format-changes";

#[derive(Serialize, Deserialize)]
struct SaveEnvelope {
    version: u32,
    snapshot: Box<RawValue>,
    mac: String,
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn compute_mac_from_raw(version: u32, snapshot_raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SAVE_SECRET);
    hasher.update(version.to_le_bytes());
    hasher.update(snapshot_raw.as_bytes());
    to_hex(&hasher.finalize())
}

pub(crate) fn save_game(path: &Path, game: &Game) -> Result<(), String> {
    let snapshot = game.snapshot();
    let snapshot_raw =
        serde_json::to_string(&snapshot).map_err(|e| format!("serialize snapshot: {e}"))?;
    let mac = compute_mac_from_raw(SAVE_VERSION, &snapshot_raw);
    let snapshot_box =
        RawValue::from_string(snapshot_raw).map_err(|e| format!("build raw snapshot: {e}"))?;
    let envelope = SaveEnvelope {
        version: SAVE_VERSION,
        snapshot: snapshot_box,
        mac,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|e| format!("serialize save: {e}"))?;
    fs::write(path, bytes).map_err(|e| format!("write save: {e}"))
}

pub(crate) fn load_game(path: &Path) -> Result<Game, String> {
    let bytes = fs::read(path).map_err(|e| format!("read save: {e}"))?;
    let envelope: SaveEnvelope =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse save: {e}"))?;

    if envelope.version != SAVE_VERSION {
        return Err("unsupported save version".to_string());
    }

    let expected_mac = compute_mac_from_raw(envelope.version, envelope.snapshot.get());
    if envelope.mac != expected_mac {
        return Err("save file verification failed (tampered or corrupted)".to_string());
    }

    let snapshot: GameSnapshot = serde_json::from_str(envelope.snapshot.get())
        .map_err(|e| format!("parse snapshot: {e}"))?;
    Game::from_snapshot(snapshot)
}

pub(crate) fn delete_save(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|e| format!("remove save: {e}"))
}
