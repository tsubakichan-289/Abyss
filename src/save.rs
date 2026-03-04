use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::game::{Game, GameSnapshot};

const SAVE_VERSION: u32 = 1;
const SAVE_SECRET: &[u8] = b"abyss-save-secret-v1-rotate-if-format-changes";

#[derive(Serialize, Deserialize)]
struct SaveEnvelope {
    version: u32,
    snapshot: GameSnapshot,
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

fn compute_mac(version: u32, snapshot: &GameSnapshot) -> Result<String, String> {
    let snapshot_bytes = serde_json::to_vec(snapshot).map_err(|e| format!("serialize snapshot: {e}"))?;
    let mut hasher = Sha256::new();
    hasher.update(SAVE_SECRET);
    hasher.update(version.to_le_bytes());
    hasher.update(snapshot_bytes);
    Ok(to_hex(&hasher.finalize()))
}

pub(crate) fn save_game(path: &Path, game: &Game) -> Result<(), String> {
    let snapshot = game.snapshot();
    let mac = compute_mac(SAVE_VERSION, &snapshot)?;
    let envelope = SaveEnvelope {
        version: SAVE_VERSION,
        snapshot,
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

    let expected_mac = compute_mac(envelope.version, &envelope.snapshot)?;
    if envelope.mac != expected_mac {
        return Err("save file verification failed (tampered or corrupted)".to_string());
    }

    Game::from_snapshot(envelope.snapshot)
}

pub(crate) fn delete_save(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|e| format!("remove save: {e}"))
}
