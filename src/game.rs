use std::collections::{HashMap, HashSet, VecDeque};
use std::f32::consts::FRAC_PI_4;
use std::sync::OnceLock;

use ratatui::prelude::Color;
use serde::{Deserialize, Serialize};

use crate::defs::{Faction, creature_meta, defs, tile_meta};
use crate::text::{tr, trf};
use crate::{Facing, InventoryItem, Item, Tile};

const LEVEL_EXP_BASE: u32 = 20;
const LEVEL_EXP_STEP: u32 = 12;
const MAX_STACK_QTY: u16 = 10;
const ENEMY_ATTACK_BASE_DELAY_FRAMES: u8 = 4;
const ENEMY_ATTACK_STAGGER_FRAMES: u8 = 6;
const ENEMY_PATHFIND_MAX_DEPTH: u8 = 24;
const ENEMY_PATHFIND_MAX_RADIUS: i32 = 24;
const STAIRS_TARGET_DISTANCE: i32 = 100;
const STAIRS_SEARCH_RADIUS: i32 = 12;
const PLAYER_BASE_MP: i32 = 10;
const LEVEL_UP_MP_GAIN: i32 = 2;
const FLAME_SCROLL_MP_COST: i32 = 2;
const BLINK_SCROLL_MP_COST: i32 = 3;
const NOVA_SCROLL_MP_COST: i32 = 5;
const PLAYER_BASE_HUNGER: i32 = 100;
const FOOD_HUNGER_RESTORE: i32 = 30;
const BREAD_HUNGER_RESTORE: i32 = 20;
const TORCH_LIGHT_RADIUS: i32 = 5;
const DARK_SPAWN_INTERVAL_TURNS: u64 = 6;
const DARK_SPAWN_RADIUS: i32 = 24;
const DARK_SPAWN_MIN_DIST2: i32 = 25;
const DARK_SPAWN_DENSITY_RADIUS: i32 = 6;
const INITIAL_TRAVELER_COUNT: usize = 2;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EffectCell {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) glyph: char,
    pub(crate) color: Color,
    pub(crate) bold: bool,
}

#[derive(Clone, Debug)]
struct ActiveVisualEffect {
    effect_id: String,
    origin: Pos,
    facing: Facing,
    style_key: String,
    delay_frames: u8,
    frame_index: u16,
    frame_tick: u8,
}

#[derive(Deserialize)]
struct EffectFileRaw {
    defaults: EffectDefaultsRaw,
    effects: HashMap<String, EffectDefRaw>,
}

#[derive(Deserialize)]
struct EffectDefaultsRaw {
    frame_duration: u8,
    transparent_char: String,
}

#[derive(Deserialize)]
struct EffectDefRaw {
    base_direction: String,
    auto_rotate: bool,
    size: u8,
    frames: Vec<Vec<String>>,
    frame_duration: Option<u8>,
    transparent_char: Option<String>,
    style_presets: Option<HashMap<String, EffectStyleRaw>>,
}

#[derive(Deserialize)]
struct EffectStyleRaw {
    fg: Option<String>,
    color_index: Option<u8>,
    bold: Option<bool>,
}

#[derive(Clone)]
struct EffectCatalog {
    effects: HashMap<String, EffectDef>,
}

#[derive(Clone)]
struct EffectDef {
    base_direction: Facing,
    auto_rotate: bool,
    size: usize,
    frames: Vec<Vec<char>>,
    frame_duration: u8,
    transparent_char: char,
    style_presets: HashMap<String, EffectStyle>,
}

#[derive(Clone, Copy)]
struct EffectStyle {
    color: Color,
    bold: bool,
}

fn effect_catalog() -> &'static EffectCatalog {
    static CATALOG: OnceLock<EffectCatalog> = OnceLock::new();
    CATALOG.get_or_init(load_effect_catalog)
}

fn parse_single_char_or(s: &str, fallback: char) -> char {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => c,
        _ => fallback,
    }
}

fn parse_named_color(name: &str) -> Option<Color> {
    match name.to_ascii_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "darkgray" | "dark_gray" | "grey" | "gray" => Some(Color::DarkGray),
        _ => None,
    }
}

fn parse_facing_key(key: &str) -> Option<Facing> {
    match key {
        "N" | "n" => Some(Facing::N),
        "NE" | "ne" => Some(Facing::NE),
        "E" | "e" => Some(Facing::E),
        "SE" | "se" => Some(Facing::SE),
        "S" | "s" => Some(Facing::S),
        "SW" | "sw" => Some(Facing::SW),
        "W" | "w" => Some(Facing::W),
        "NW" | "nw" => Some(Facing::NW),
        _ => None,
    }
}

fn facing_steps_45(facing: Facing) -> i32 {
    match facing {
        Facing::E => 0,
        Facing::SE => 1,
        Facing::S => 2,
        Facing::SW => 3,
        Facing::W => 4,
        Facing::NW => 5,
        Facing::N => 6,
        Facing::NE => 7,
    }
}

fn rotate_point_45(x: i32, y: i32, steps: i32) -> (i32, i32) {
    let theta = (steps as f32) * FRAC_PI_4;
    let cos_t = theta.cos();
    let sin_t = theta.sin();
    let fx = x as f32;
    let fy = y as f32;
    let rx = (fx * cos_t - fy * sin_t).round() as i32;
    let ry = (fx * sin_t + fy * cos_t).round() as i32;
    (rx, ry)
}

fn rotate_frame(base: &[char], size: usize, base_dir: Facing, target_dir: Facing) -> Vec<char> {
    let mut out = vec!['\0'; size * size];
    let center = (size / 2) as i32;
    let steps = facing_steps_45(target_dir) - facing_steps_45(base_dir);
    for y in 0..size {
        for x in 0..size {
            let ch = base[y * size + x];
            let ox = x as i32 - center;
            let oy = y as i32 - center;
            let (rx, ry) = rotate_point_45(ox, oy, steps);
            let nx = rx + center;
            let ny = ry + center;
            if nx < 0 || ny < 0 {
                continue;
            }
            let ux = nx as usize;
            let uy = ny as usize;
            if ux >= size || uy >= size {
                continue;
            }
            out[uy * size + ux] = ch;
        }
    }
    out
}

fn oriented_tip_glyph(base_glyph: char, facing: Facing) -> char {
    if base_glyph != '>' {
        return base_glyph;
    }
    match facing {
        Facing::E => '>',
        Facing::W => '<',
        Facing::N => '^',
        Facing::S => 'v',
        Facing::NE | Facing::SW => '/',
        Facing::NW | Facing::SE => '\\',
    }
}

fn load_effect_catalog() -> EffectCatalog {
    let raw: EffectFileRaw = serde_json::from_str(include_str!("../data/effect.json"))
        .expect("failed to parse data/effect.json");
    let default_frame_duration = raw.defaults.frame_duration.max(1);
    let default_transparent_char = parse_single_char_or(&raw.defaults.transparent_char, '.');

    let mut effects = HashMap::new();
    for (id, def) in raw.effects {
        let base_direction = parse_facing_key(&def.base_direction).unwrap_or(Facing::E);
        let size = def.size.max(1) as usize;
        let frame_duration = def.frame_duration.unwrap_or(default_frame_duration).max(1);
        let transparent_char = def
            .transparent_char
            .as_deref()
            .map(|s| parse_single_char_or(s, default_transparent_char))
            .unwrap_or(default_transparent_char);
        let mut frames: Vec<Vec<char>> = Vec::new();
        for (fi, rows) in def.frames.iter().enumerate() {
            assert!(
                rows.len() == size,
                "effect '{}' frame {} must have {} rows",
                id,
                fi,
                size
            );
            let mut flat = Vec::with_capacity(size * size);
            for (ri, row) in rows.iter().enumerate() {
                let chars: Vec<char> = row.chars().collect();
                assert!(
                    chars.len() == size,
                    "effect '{}' frame {} row {} must have {} chars",
                    id,
                    fi,
                    ri,
                    size
                );
                flat.extend(chars);
            }
            frames.push(flat);
        }
        assert!(
            !frames.is_empty(),
            "effect '{}' must have at least one frame",
            id
        );

        let mut style_presets: HashMap<String, EffectStyle> = HashMap::new();
        if let Some(raw_presets) = def.style_presets {
            for (k, v) in raw_presets {
                let color = if let Some(i) = v.color_index {
                    Color::Indexed(i)
                } else if let Some(name) = v.fg {
                    parse_named_color(&name).unwrap_or(Color::Yellow)
                } else {
                    Color::Yellow
                };
                style_presets.insert(
                    k,
                    EffectStyle {
                        color,
                        bold: v.bold.unwrap_or(true),
                    },
                );
            }
        }
        effects.insert(
            id,
            EffectDef {
                base_direction,
                auto_rotate: def.auto_rotate,
                size,
                frames,
                frame_duration,
                transparent_char,
                style_presets,
            },
        );
    }

    EffectCatalog { effects }
}

fn exp_needed_for_level(level: u32) -> u32 {
    LEVEL_EXP_BASE + (level.saturating_sub(1) * LEVEL_EXP_STEP)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Move(i32, i32),
    Face(i32, i32),
    Attack,
    Wait,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pos {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct Enemy {
    pub(crate) id: u64,
    pub(crate) pos: Pos,
    pub(crate) hp: i32,
    pub(crate) creature_id: String,
    facing: Facing,
    flee_from_player: bool,
}

#[derive(Clone, Copy, Debug)]
struct HarvestState {
    target: (i32, i32),
    hits: u8,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AttackEffect {
    pub(crate) from: Pos,
    pub(crate) to: Pos,
    pub(crate) delay_frames: u8,
    pub(crate) ttl_frames: u8,
}

#[derive(Clone, Debug)]
struct PendingEnemyHit {
    enemy_name: String,
    damage: i32,
    delay_frames: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BiomeId(u8);

impl BiomeId {
    fn new(v: u8) -> Self {
        Self(v.min(15))
    }
}

struct BiomeProfile {
    elevation_bias: f64,
    abyss_shift: f64,
    rock_shift: f64,
    wall_shift: f64,
}

fn biome_name(id: BiomeId) -> &'static str {
    let fallback = match id.0 {
        0 => "Ash Barrens",
        1 => "Salt Marsh",
        2 => "Green Fields",
        3 => "Luminous Grove",
        4 => "Frost Flats",
        5 => "Fog Meadow",
        6 => "Woodland",
        7 => "Rainforest",
        8 => "Stone Steppe",
        9 => "Highland",
        10 => "Alpine Forest",
        11 => "Cloud Ridge",
        12 => "Obsidian Waste",
        13 => "Crag Depths",
        14 => "Spire Peaks",
        _ => "Elder Summit",
    };
    let key = format!("biome.name.{}", id.0);
    let val = tr(&key);
    if val == key {
        fallback
    } else {
        val
    }
}

fn biome_enemy_multiplier(biome: BiomeId, creature_id: &str) -> u32 {
    let bx = biome.0 % 4;
    let by = biome.0 / 4;
    let edge = bx == 0 || bx == 3 || by == 0 || by == 3;
    match creature_id {
        "slime" | "slime_brute" => match (bx, by) {
            (1, 0) | (2, 0) | (1, 1) | (2, 1) => 220,
            (1, 2) | (2, 2) => 140,
            _ => 70,
        },
        "wolf" | "wolf_alpha" => match (bx, by) {
            (2, 1) | (3, 1) | (2, 2) | (3, 2) => 220,
            (1, 1) | (1, 2) => 130,
            _ => 55,
        },
        "bat" | "bat_night" => match (bx, by) {
            (2, 2) | (3, 2) | (2, 3) | (3, 3) => 220,
            (1, 2) | (1, 3) => 125,
            _ => 55,
        },
        "golem" | "golem_elder" => {
            if edge {
                190
            } else {
                75
            }
        }
        _ => 100,
    }
}

fn biome_item_pool(biome: BiomeId) -> &'static [(Item, u32)] {
    let bx = biome.0 % 4;
    let by = biome.0 / 4;
    match (bx, by) {
        // wetter, softer
        (0, 0) | (1, 0) | (0, 1) => &[
            (Item::StringFiber, 40),
            (Item::Food, 20),
            (Item::Torch, 8),
            (Item::Herb, 30),
            (Item::Potion, 15),
            (Item::FlameScroll, 3),
            (Item::BlinkScroll, 2),
            (Item::NovaScroll, 1),
            (Item::Wood, 10),
            (Item::Hide, 3),
        ],
        // greener
        (2, 1) | (1, 2) | (2, 2) => &[
            (Item::Wood, 38),
            (Item::Food, 18),
            (Item::Torch, 7),
            (Item::StringFiber, 20),
            (Item::Herb, 18),
            (Item::Potion, 14),
            (Item::FlameScroll, 4),
            (Item::BlinkScroll, 3),
            (Item::NovaScroll, 1),
            (Item::Hide, 3),
        ],
        // rocky / high
        (3, 2) | (2, 3) | (3, 3) => &[
            (Item::Stone, 36),
            (Item::Food, 10),
            (Item::Torch, 6),
            (Item::IronIngot, 28),
            (Item::Potion, 14),
            (Item::FlameScroll, 4),
            (Item::BlinkScroll, 4),
            (Item::NovaScroll, 2),
            (Item::Wood, 8),
            (Item::Elixir, 6),
            (Item::Herb, 6),
        ],
        // mixed
        _ => &[
            (Item::Potion, 20),
            (Item::Food, 14),
            (Item::Torch, 6),
            (Item::Wood, 24),
            (Item::Stone, 22),
            (Item::StringFiber, 12),
            (Item::Herb, 12),
            (Item::IronIngot, 6),
            (Item::FlameScroll, 3),
            (Item::BlinkScroll, 2),
            (Item::NovaScroll, 1),
            (Item::Hide, 2),
        ],
    }
}

fn is_bright_by_facing(facing: Facing, dx: i32, dy: i32) -> bool {
    if dx == 0 && dy == 0 {
        return true;
    }
    let (fx, fy) = facing.delta();
    let dot = (fx * dx + fy * dy) as f32;
    let mag = ((dx * dx + dy * dy) as f32).sqrt();
    if mag == 0.0 {
        return true;
    }
    let cos = dot / mag;
    cos >= 0.35
}

fn rotate_facing_clockwise(facing: Facing) -> Facing {
    match facing {
        Facing::N => Facing::NE,
        Facing::NE => Facing::E,
        Facing::E => Facing::SE,
        Facing::SE => Facing::S,
        Facing::S => Facing::SW,
        Facing::SW => Facing::W,
        Facing::W => Facing::NW,
        Facing::NW => Facing::N,
    }
}

fn biome_profile(id: BiomeId) -> BiomeProfile {
    let x = (id.0 % 4) as f64;
    let y = (id.0 / 4) as f64;
    let nx = (x - 1.5) / 1.5;
    let ny = (y - 1.5) / 1.5;
    let edge = nx.abs().max(ny.abs());
    let rugged = (nx * 0.65 + ny * 0.35).clamp(-1.0, 1.0);
    let wet = (ny * 0.8 - nx * 0.25).clamp(-1.0, 1.0);
    BiomeProfile {
        elevation_bias: rugged * 0.06 + wet * 0.02,
        abyss_shift: (-wet * 0.04 + edge * 0.03).clamp(-0.06, 0.08),
        rock_shift: (-rugged * 0.04 - edge * 0.05).clamp(-0.11, 0.08),
        wall_shift: (-rugged * 0.03 - edge * 0.04).clamp(-0.08, 0.06),
    }
}

#[derive(Clone)]
struct Chunk {
    tiles: [Tile; crate::CHUNK_AREA],
    biome: BiomeId,
    biome_noise_a: f64,
    biome_noise_b: f64,
}

impl Chunk {
    fn new(fill: Tile, biome: BiomeId, biome_noise_a: f64, biome_noise_b: f64) -> Self {
        Self {
            tiles: [fill; crate::CHUNK_AREA],
            biome,
            biome_noise_a,
            biome_noise_b,
        }
    }

    fn idx(local_x: usize, local_y: usize) -> usize {
        local_y * crate::CHUNK_SIZE + local_x
    }

    fn get(&self, local_x: usize, local_y: usize) -> Tile {
        self.tiles[Self::idx(local_x, local_y)]
    }

    fn set(&mut self, local_x: usize, local_y: usize, tile: Tile) {
        let idx = Self::idx(local_x, local_y);
        self.tiles[idx] = tile;
    }
}

pub(crate) struct World {
    pub(crate) seed: u64,
    chunks: HashMap<(i32, i32), Chunk>,
}

impl World {
    fn new(seed: u64) -> Self {
        Self {
            seed,
            chunks: HashMap::new(),
        }
    }

    fn chunk_coord(v: i32) -> i32 {
        v.div_euclid(crate::CHUNK_SIZE as i32)
    }

    fn local_coord(v: i32) -> usize {
        v.rem_euclid(crate::CHUNK_SIZE as i32) as usize
    }

    fn tile(&mut self, x: i32, y: i32) -> Tile {
        let chunk_x = Self::chunk_coord(x);
        let chunk_y = Self::chunk_coord(y);
        let local_x = Self::local_coord(x);
        let local_y = Self::local_coord(y);
        let chunk = self.ensure_chunk(chunk_x, chunk_y);
        chunk.get(local_x, local_y)
    }

    fn set_tile(&mut self, x: i32, y: i32, tile: Tile) {
        let chunk_x = Self::chunk_coord(x);
        let chunk_y = Self::chunk_coord(y);
        let local_x = Self::local_coord(x);
        let local_y = Self::local_coord(y);
        let chunk = self.ensure_chunk(chunk_x, chunk_y);
        chunk.set(local_x, local_y, tile);
    }

    fn ensure_chunk(&mut self, chunk_x: i32, chunk_y: i32) -> &mut Chunk {
        self.chunks
            .entry((chunk_x, chunk_y))
            .or_insert_with(|| Self::generate_chunk(self.seed, chunk_x, chunk_y))
    }

    fn quantize_biome_axis(v: f64) -> u8 {
        if v < 0.15 {
            0
        } else if v < 0.50 {
            1
        } else if v < 0.85 {
            2
        } else {
            3
        }
    }

    fn generate_chunk(seed: u64, chunk_x: i32, chunk_y: i32) -> Chunk {
        let biome_noise_a_gen = crate::noise::Perlin2D::new(seed ^ 0x9E37_79B9_AA55_AA55);
        let biome_noise_b_gen = crate::noise::Perlin2D::new(seed ^ 0xC2B2_AE35_1234_5678);
        let biome_a = biome_noise_a_gen.noise01(chunk_x as f64 * 0.19, chunk_y as f64 * 0.19);
        let biome_b =
            biome_noise_b_gen.noise01(chunk_x as f64 * 0.19 + 111.7, chunk_y as f64 * 0.19 - 77.3);
        let bx = Self::quantize_biome_axis(biome_a);
        let by = Self::quantize_biome_axis(biome_b);
        let biome = BiomeId::new(by * 4 + bx);
        let profile = biome_profile(biome);

        let mut chunk = Chunk::new(Tile::DeepWater, biome, biome_a, biome_b);
        let terrain_noise = crate::noise::Perlin2D::new(seed);
        let scale = 0.05;
        let octaves = 4;
        let persistence = 0.5;
        let lacunarity = 2.0;

        let base_x = chunk_x * crate::CHUNK_SIZE as i32;
        let base_y = chunk_y * crate::CHUNK_SIZE as i32;

        for local_y in 0..crate::CHUNK_SIZE {
            for local_x in 0..crate::CHUNK_SIZE {
                let world_x = base_x + local_x as i32;
                let world_y = base_y + local_y as i32;
                let h = crate::fbm_noise01(
                    &terrain_noise,
                    world_x as f64 * scale,
                    world_y as f64 * scale,
                    octaves,
                    persistence,
                    lacunarity,
                );
                let h = (h + profile.elevation_bias).clamp(0.0, 1.0);
                let abyss_threshold = (crate::ABYSS_THRESHOLD + profile.abyss_shift).clamp(0.03, 0.48);
                let rock_threshold =
                    (crate::ROCK_THRESHOLD + profile.rock_shift).clamp(0.48, 0.90);
                let wall_threshold =
                    (crate::WALL_THRESHOLD + profile.wall_shift).clamp(rock_threshold + 0.02, 0.96);

                let tile = if h <= abyss_threshold {
                    Tile::Abyss
                } else if h >= wall_threshold {
                    Tile::Wall
                } else if h >= rock_threshold {
                    Tile::Rock
                } else {
                    Tile::from_height(h)
                };

                chunk.set(local_x, local_y, tile);
            }
        }

        chunk
    }

    fn biome_name_at(&mut self, x: i32, y: i32) -> &'static str {
        let chunk_x = Self::chunk_coord(x);
        let chunk_y = Self::chunk_coord(y);
        let chunk = self.ensure_chunk(chunk_x, chunk_y);
        biome_name(chunk.biome)
    }

    fn biome_id_at(&mut self, x: i32, y: i32) -> BiomeId {
        let chunk_x = Self::chunk_coord(x);
        let chunk_y = Self::chunk_coord(y);
        let chunk = self.ensure_chunk(chunk_x, chunk_y);
        chunk.biome
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveResult {
    Moved,
    Blocked,
    RotatedOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GameSnapshot {
    seed: u64,
    chunks: Vec<ChunkState>,
    player: PosState,
    #[serde(default = "default_player_id")]
    player_id: u64,
    facing: Facing,
    player_hp: i32,
    player_max_hp: i32,
    #[serde(default = "default_player_mp")]
    player_mp: i32,
    #[serde(default = "default_player_max_mp")]
    player_max_mp: i32,
    #[serde(default = "default_player_hunger")]
    player_hunger: i32,
    #[serde(default = "default_player_max_hunger")]
    player_max_hunger: i32,
    inventory: Vec<InventoryItem>,
    #[serde(default)]
    equipped_sword: Option<InventoryItem>,
    #[serde(default)]
    equipped_shield: Option<InventoryItem>,
    #[serde(default)]
    equipped_accessory: Option<InventoryItem>,
    enemies: Vec<EnemyState>,
    ground_items: Vec<GroundItemState>,
    #[serde(default)]
    blood_stains: Vec<PosState>,
    #[serde(default)]
    torches: Vec<TorchState>,
    harvest_state: Option<HarvestStateState>,
    rng_state: u64,
    turn: u64,
    #[serde(default = "default_floor")]
    floor: u32,
    #[serde(default = "default_level")]
    level: u32,
    #[serde(default)]
    exp: u32,
    #[serde(default = "default_next_exp")]
    next_exp: u32,
    #[serde(default = "default_next_entity_id")]
    next_entity_id: u64,
    #[serde(default)]
    stat_enemies_defeated: u32,
    #[serde(default)]
    stat_damage_dealt: u32,
    #[serde(default)]
    stat_damage_taken: u32,
    #[serde(default)]
    stat_items_picked: u32,
    #[serde(default)]
    stat_steps: u32,
    #[serde(default)]
    stat_total_exp: u32,
    logs: Vec<String>,
}

fn default_level() -> u32 {
    1
}

fn default_floor() -> u32 {
    1
}

fn default_next_exp() -> u32 {
    exp_needed_for_level(1)
}

fn default_player_id() -> u64 {
    0
}

fn default_player_mp() -> i32 {
    PLAYER_BASE_MP
}

fn default_player_max_mp() -> i32 {
    PLAYER_BASE_MP
}

fn default_player_hunger() -> i32 {
    PLAYER_BASE_HUNGER
}

fn default_player_max_hunger() -> i32 {
    PLAYER_BASE_HUNGER
}

fn default_next_entity_id() -> u64 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChunkState {
    chunk_x: i32,
    chunk_y: i32,
    tiles: Vec<Tile>,
    biome: u8,
    biome_noise_a: f64,
    biome_noise_b: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct PosState {
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EnemyState {
    #[serde(default)]
    id: u64,
    pos: PosState,
    hp: i32,
    creature_id: String,
    #[serde(default = "default_enemy_facing")]
    facing: Facing,
    #[serde(default)]
    flee_from_player: bool,
}

fn default_enemy_facing() -> Facing {
    Facing::S
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct GroundItemState {
    x: i32,
    y: i32,
    item: Item,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct TorchState {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct HarvestStateState {
    x: i32,
    y: i32,
    hits: u8,
}

pub(crate) struct Game {
    pub(crate) world: World,
    pub(crate) player: Pos,
    player_id: u64,
    pub(crate) facing: Facing,
    pub(crate) player_hp: i32,
    pub(crate) player_max_hp: i32,
    pub(crate) player_mp: i32,
    pub(crate) player_max_mp: i32,
    pub(crate) player_hunger: i32,
    pub(crate) player_max_hunger: i32,
    pub(crate) inventory: Vec<InventoryItem>,
    pub(crate) equipped_sword: Option<InventoryItem>,
    pub(crate) equipped_shield: Option<InventoryItem>,
    pub(crate) equipped_accessory: Option<InventoryItem>,
    pub(crate) enemies: Vec<Enemy>,
    pub(crate) ground_items: HashMap<(i32, i32), Item>,
    blood_stains: HashSet<(i32, i32)>,
    torches: HashSet<(i32, i32)>,
    pub(crate) attack_effects: Vec<AttackEffect>,
    visual_effects: Vec<ActiveVisualEffect>,
    pending_enemy_hits: Vec<PendingEnemyHit>,
    harvest_state: Option<HarvestState>,
    rng_state: u64,
    next_entity_id: u64,
    pub(crate) turn: u64,
    pub(crate) floor: u32,
    pub(crate) level: u32,
    pub(crate) exp: u32,
    pub(crate) next_exp: u32,
    pub(crate) stat_enemies_defeated: u32,
    pub(crate) stat_damage_dealt: u32,
    pub(crate) stat_damage_taken: u32,
    pub(crate) stat_items_picked: u32,
    pub(crate) stat_steps: u32,
    pub(crate) stat_total_exp: u32,
    pub(crate) logs: Vec<String>,
    pending_dialogue: Option<String>,
    invincible: bool,
}

impl Game {
    pub(crate) fn new(seed: u64) -> Self {
        let mut game = Self {
            world: World::new(seed),
            player: Pos { x: 0, y: 0 },
            player_id: 0,
            facing: Facing::S,
            player_hp: creature_meta("player").hp,
            player_max_hp: creature_meta("player").hp,
            player_mp: PLAYER_BASE_MP,
            player_max_mp: PLAYER_BASE_MP,
            player_hunger: PLAYER_BASE_HUNGER,
            player_max_hunger: PLAYER_BASE_HUNGER,
            inventory: Vec::new(),
            equipped_sword: None,
            equipped_shield: None,
            equipped_accessory: None,
            enemies: Vec::new(),
            ground_items: HashMap::new(),
            blood_stains: HashSet::new(),
            torches: HashSet::new(),
            attack_effects: Vec::new(),
            visual_effects: Vec::new(),
            pending_enemy_hits: Vec::new(),
            harvest_state: None,
            rng_state: seed ^ 0xA5A5_5A5A_DEAD_BEEF,
            next_entity_id: 1,
            turn: 0,
            floor: 1,
            level: 1,
            exp: 0,
            next_exp: exp_needed_for_level(1),
            stat_enemies_defeated: 0,
            stat_damage_dealt: 0,
            stat_damage_taken: 0,
            stat_items_picked: 0,
            stat_steps: 0,
            stat_total_exp: 0,
            logs: vec![tr("game.start").to_string()],
            pending_dialogue: None,
            invincible: false,
        };
        game.player = game.find_spawn();
        game.place_initial_stairs();
        game.spawn_enemies(12);
        game.spawn_travelers(INITIAL_TRAVELER_COUNT);
        game.spawn_items(10);
        game
    }

    pub(crate) fn tile(&mut self, x: i32, y: i32) -> Tile {
        self.world.tile(x, y)
    }

    fn set_tile(&mut self, x: i32, y: i32, tile: Tile) {
        self.world.set_tile(x, y, tile);
    }

    fn next_floor_seed(&self) -> u64 {
        let salt = 0x9E37_79B9_7F4A_7C15_u64;
        self.world
            .seed
            .wrapping_add(salt)
            .rotate_left((self.floor % 63 + 1) as u32)
    }

    pub(crate) fn is_on_stairs(&mut self) -> bool {
        self.tile(self.player.x, self.player.y) == Tile::StairsDown
    }

    pub(crate) fn descend_floor(&mut self) {
        self.floor = self.floor.saturating_add(1);
        let next_seed = self.next_floor_seed();
        self.world = World::new(next_seed);
        self.ground_items.clear();
        self.blood_stains.clear();
        self.torches.clear();
        self.enemies.clear();
        self.attack_effects.clear();
        self.visual_effects.clear();
        self.pending_enemy_hits.clear();
        self.harvest_state = None;
        self.player_mp = self.player_max_mp;
        self.player = self.find_spawn();
        self.place_initial_stairs();
        self.spawn_enemies(12);
        self.spawn_travelers(INITIAL_TRAVELER_COUNT);
        self.spawn_items(10);
        self.push_log(trf(
            "game.descended_floor",
            &[("floor", self.floor.to_string())],
        ));
    }

    fn find_walkable_near(&mut self, cx: i32, cy: i32, max_radius: i32) -> Option<(i32, i32)> {
        for r in 0..=max_radius {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let x = cx + dx;
                    let y = cy + dy;
                    if x == self.player.x && y == self.player.y {
                        continue;
                    }
                    if self.tile(x, y).walkable() {
                        return Some((x, y));
                    }
                }
            }
        }
        None
    }

    fn place_initial_stairs(&mut self) {
        let d = STAIRS_TARGET_DISTANCE;
        for _ in 0..512 {
            let side = self.rand_u32() % 4;
            let offset = self.rand_range_i32(-d, d);
            let (dx, dy) = match side {
                0 => (offset, -d),
                1 => (offset, d),
                2 => (-d, offset),
                _ => (d, offset),
            };
            let cx = self.player.x + dx;
            let cy = self.player.y + dy;
            if let Some((sx, sy)) = self.find_walkable_near(cx, cy, STAIRS_SEARCH_RADIUS) {
                self.set_tile(sx, sy, Tile::StairsDown);
                return;
            }
        }

        // Fallback: if no distant walkable tile is found, place near spawn.
        if let Some((sx, sy)) = self.find_walkable_near(self.player.x, self.player.y, 3) {
            self.set_tile(sx, sy, Tile::StairsDown);
        }
    }

    fn alloc_entity_id(&mut self) -> u64 {
        let id = self.next_entity_id.max(1);
        self.next_entity_id = id.saturating_add(1);
        id
    }

    fn find_spawn(&mut self) -> Pos {
        if self.tile(0, 0).walkable() {
            return Pos { x: 0, y: 0 };
        }

        for radius in 1..=128_i32 {
            for y in -radius..=radius {
                for x in -radius..=radius {
                    if self.tile(x, y).walkable() {
                        return Pos { x, y };
                    }
                }
            }
        }

        Pos { x: 0, y: 0 }
    }

    fn try_move(&mut self, dx: i32, dy: i32) -> MoveResult {
        let old_facing = self.facing;
        if let Some(facing) = Facing::from_delta(dx, dy) {
            self.facing = facing;
        }
        let nx = self.player.x + dx;
        let ny = self.player.y + dy;
        if let Some(i) = self.enemies.iter().position(|e| e.pos.x == nx && e.pos.y == ny) {
            if creature_meta(&self.enemies[i].creature_id).faction == Faction::Neutral {
                let cid = self.enemies[i].creature_id.clone();
                if self.enemies[i].flee_from_player {
                    self.queue_neutral_flee_dialogue(&cid);
                } else {
                    self.queue_neutral_talk_dialogue(&cid);
                }
                if self.facing != old_facing {
                    return MoveResult::RotatedOnly;
                }
                return MoveResult::Blocked;
            }
            if self.facing != old_facing {
                return MoveResult::RotatedOnly;
            }
            self.push_log(tr("game.enemy_blocks"));
            return MoveResult::Blocked;
        }
        if self.tile(nx, ny).walkable() {
            self.player = Pos { x: nx, y: ny };
            self.stat_steps = self.stat_steps.saturating_add(1);
            self.pick_up_item_at_player();
            MoveResult::Moved
        } else {
            if self.facing != old_facing {
                return MoveResult::RotatedOnly;
            }
            MoveResult::Blocked
        }
    }

    pub(crate) fn apply_action(&mut self, action: Action) {
        let mut consume_turn = true;
        let mut keep_harvest_chain = false;
        match action {
            Action::Move(dx, dy) => {
                let result = self.try_move(dx, dy);
                if result != MoveResult::Moved {
                    consume_turn = false;
                }
            }
            Action::Face(dx, dy) => {
                if let Some(facing) = Facing::from_delta(dx, dy) {
                    if facing != self.facing {
                        self.facing = facing;
                    }
                }
                consume_turn = false;
            }
            Action::Attack => {
                keep_harvest_chain = self.player_attack();
            }
            Action::Wait => {
                self.push_log(tr("game.waited"));
            }
        }
        if consume_turn {
            if !keep_harvest_chain {
                self.harvest_state = None;
            }
            self.consume_turn();
        }
    }

    pub(crate) fn push_log<S: Into<String>>(&mut self, msg: S) {
        self.logs.push(msg.into());
        if self.logs.len() > 300 {
            self.logs.drain(0..100);
        }
    }

    fn queue_neutral_dialogue_with_suffix(&mut self, creature_id: &str, suffix: &str) {
        let key = format!("dialogue.{}.{}", creature_id, suffix);
        let line = tr(&key);
        let speaker = crate::localized_creature_name(creature_id);
        if line == key {
            let fallback = tr(&format!("dialogue.neutral.{}", suffix));
            self.pending_dialogue = Some(format!("{speaker}:{fallback}"));
        } else {
            self.pending_dialogue = Some(format!("{speaker}:{line}"));
        }
    }

    fn queue_neutral_attacked_dialogue(&mut self, creature_id: &str) {
        self.queue_neutral_dialogue_with_suffix(creature_id, "attacked");
    }

    fn queue_neutral_talk_dialogue(&mut self, creature_id: &str) {
        self.queue_neutral_dialogue_with_suffix(creature_id, "talk");
    }

    fn queue_neutral_flee_dialogue(&mut self, creature_id: &str) {
        self.queue_neutral_dialogue_with_suffix(creature_id, "flee");
    }

    fn push_death_cry(&mut self, creature_id: &str) {
        if creature_meta(creature_id).faction != Faction::Neutral {
            return;
        }
        let key = format!("dialogue.{}.death", creature_id);
        let line = tr(&key);
        let speaker = crate::localized_creature_name(creature_id);
        if line == key {
            let fallback = tr("dialogue.generic.death");
            self.push_log(format!("{speaker}:{fallback}"));
        } else {
            self.push_log(format!("{speaker}:{line}"));
        }
    }

    pub(crate) fn take_pending_dialogue(&mut self) -> Option<String> {
        self.pending_dialogue.take()
    }

    pub(crate) fn set_invincible(&mut self, enabled: bool) {
        self.invincible = enabled;
    }

    pub(crate) fn invincible(&self) -> bool {
        self.invincible
    }

    fn player_attack(&mut self) -> bool {
        let (dx, dy) = self.facing.delta();
        let tx = self.player.x + dx;
        let ty = self.player.y + dy;
        self.push_attack_effect(self.player, Pos { x: tx, y: ty }, 0);
        let target_idx = self
            .enemies
            .iter()
            .position(|e| e.pos.x == tx && e.pos.y == ty);

        match target_idx {
            Some(i) => {
                let was_neutral =
                    creature_meta(&self.enemies[i].creature_id).faction == Faction::Neutral;
                let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
                let damage = calc_damage(self.player_attack_power(), enemy_def);
                self.stat_damage_dealt = self.stat_damage_dealt.saturating_add(damage as u32);
                self.enemies[i].hp -= damage;
                if Self::is_traveler_id(&self.enemies[i].creature_id) {
                    self.enemies[i].flee_from_player = true;
                }
                if was_neutral && self.enemies[i].hp > 0 {
                    let cid = self.enemies[i].creature_id.clone();
                    self.queue_neutral_attacked_dialogue(&cid);
                }
                let enemy_name = crate::localized_creature_name(&self.enemies[i].creature_id);
                if self.enemies[i].hp <= 0 {
                    self.stat_enemies_defeated = self.stat_enemies_defeated.saturating_add(1);
                    let dead = self.enemies.remove(i);
                    self.blood_stains.insert((dead.pos.x, dead.pos.y));
                    self.push_death_cry(&dead.creature_id);
                    self.push_log(trf(
                        "game.you_defeated",
                        &[("enemy", enemy_name)],
                    ));
                    let cdef = creature_meta(&dead.creature_id);
                    let gained = (cdef.hp.max(1) + cdef.attack + cdef.defense * 2).max(1) as u32;
                    self.gain_exp(gained);
                    if Self::is_traveler_id(&dead.creature_id) {
                        self.maybe_drop_traveler_bread(dead.pos);
                    } else if self.rand_u32() % 100 < 60 {
                        let drop = match self.rand_u32() % 100 {
                            0..=29 => Item::Potion,
                            30..=59 => Item::Herb,
                            60..=73 => Item::Hide,
                            74..=85 => Item::IronIngot,
                            86..=92 => Item::FlameScroll,
                            93..=95 => Item::BlinkScroll,
                            96..=98 => Item::NovaScroll,
                            _ => Item::Elixir,
                        };
                        let _ = self.place_ground_item_near(dead.pos.x, dead.pos.y, drop);
                        self.push_log(trf(
                            "game.enemy_drop_item",
                            &[("item", crate::localized_item_name(drop))],
                        ));
                    }
                } else {
                    self.push_log(trf(
                        "game.you_hit_enemy",
                        &[("enemy", enemy_name), ("damage", damage.to_string())],
                    ));
                }
                false
            }
            None => {
                if self.has_torch_at(tx, ty) {
                    let label = tr("object.torch");
                    let durability = 2_u8;
                    let mut hits = 1_u8;
                    if let Some(state) = self.harvest_state {
                        if state.target == (tx, ty) {
                            hits = state.hits.saturating_add(1);
                        }
                    }
                    if hits >= durability {
                        self.torches.remove(&(tx, ty));
                        self.harvest_state = None;
                        if self.item_at(tx, ty).is_none() {
                            self.ground_items.insert((tx, ty), Item::Torch);
                            self.push_log(trf(
                                "game.broke_to_item",
                                &[
                                    ("target", label.to_string()),
                                    ("item", crate::localized_item_name(Item::Torch)),
                                ],
                            ));
                        } else {
                            self.push_log(trf(
                                "game.broke",
                                &[("target", label.to_string())],
                            ));
                        }
                    } else {
                        self.harvest_state = Some(HarvestState {
                            target: (tx, ty),
                            hits,
                        });
                        self.push_log(trf(
                            "game.damaged",
                            &[
                                ("target", label.to_string()),
                                ("hits", hits.to_string()),
                                ("max", durability.to_string()),
                            ],
                        ));
                    }
                    return true;
                }
                let target_tile = self.tile(tx, ty);
                if let Some((durability, drop_item, drop_chance, replace_to, label)) =
                    destructible_info(target_tile)
                {
                    let mut hits = 1_u8;
                    if let Some(state) = self.harvest_state {
                        if state.target == (tx, ty) {
                            hits = state.hits.saturating_add(1);
                        }
                    }
                    if hits >= durability {
                        self.set_tile(tx, ty, replace_to);
                        self.harvest_state = None;
                        if let Some(item) = drop_item {
                            if self.item_at(tx, ty).is_none()
                                && self.rand_u32() % 100 < drop_chance as u32
                            {
                                self.ground_items.insert((tx, ty), item);
                                self.push_log(trf(
                                    "game.broke_to_item",
                                    &[
                                        ("target", label.to_string()),
                                        ("item", crate::localized_item_name(item)),
                                    ],
                                ));
                            } else {
                                self.push_log(trf(
                                    "game.broke",
                                    &[("target", label.to_string())],
                                ));
                            }
                        } else {
                            self.push_log(trf(
                                "game.broke",
                                &[("target", label.to_string())],
                            ));
                        }
                    } else {
                        self.harvest_state = Some(HarvestState {
                            target: (tx, ty),
                            hits,
                        });
                        self.push_log(trf(
                            "game.damaged",
                            &[
                                ("target", label.to_string()),
                                ("hits", hits.to_string()),
                                ("max", durability.to_string()),
                            ],
                        ));
                    }
                    true
                } else {
                    self.push_log(tr("game.no_target"));
                    false
                }
            }
        }
    }

    pub(crate) fn has_enemy_at(&self, x: i32, y: i32) -> bool {
        self.enemies.iter().any(|e| e.pos.x == x && e.pos.y == y)
    }

    pub(crate) fn teleport_player(&mut self, x: i32, y: i32) -> Result<(), String> {
        if !self.tile(x, y).walkable() {
            return Err(tr("debug.tp_blocked").to_string());
        }
        if self.has_enemy_at(x, y) {
            return Err(tr("debug.tp_enemy").to_string());
        }
        self.player = Pos { x, y };
        self.harvest_state = None;
        self.pick_up_item_at_player();
        Ok(())
    }

    pub(crate) fn find_nearest_stairs(&mut self, max_radius: i32) -> Option<Pos> {
        for r in 0..=max_radius.max(0) {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let x = self.player.x + dx;
                    let y = self.player.y + dy;
                    if self.tile(x, y) == Tile::StairsDown {
                        return Some(Pos { x, y });
                    }
                }
            }
        }
        None
    }

    pub(crate) fn enemy_visual_at(&self, x: i32, y: i32) -> Option<(char, Color)> {
        self.enemies
            .iter()
            .find(|e| e.pos.x == x && e.pos.y == y)
            .map(|e| {
                let meta = creature_meta(&e.creature_id);
                (meta.glyph, meta.color)
            })
    }

    pub(crate) fn item_at(&self, x: i32, y: i32) -> Option<Item> {
        self.ground_items.get(&(x, y)).copied()
    }

    pub(crate) fn has_blood_stain(&self, x: i32, y: i32) -> bool {
        self.blood_stains.contains(&(x, y))
    }

    pub(crate) fn has_torch_at(&self, x: i32, y: i32) -> bool {
        self.torches.contains(&(x, y))
    }

    pub(crate) fn is_lit_by_torch(&self, x: i32, y: i32) -> bool {
        let r2 = TORCH_LIGHT_RADIUS * TORCH_LIGHT_RADIUS;
        self.torches.iter().any(|&(tx, ty)| {
            let dx = tx - x;
            let dy = ty - y;
            dx * dx + dy * dy <= r2
        })
    }

    pub(crate) fn inventory_len(&self) -> usize {
        self.inventory.len()
    }

    pub(crate) fn inventory_item_name(&self, idx: usize) -> Option<String> {
        self.inventory.get(idx).map(InventoryItem::display_name)
    }

    fn is_stackable_material(kind: Item) -> bool {
        matches!(
            kind,
            Item::Wood | Item::Stone | Item::StringFiber | Item::IronIngot | Item::Hide
        )
    }

    fn can_stack(item: &InventoryItem) -> bool {
        Self::is_stackable_material(item.kind) && item.custom_name.is_none()
    }

    pub(crate) fn take_inventory_one(&mut self, idx: usize) -> Option<InventoryItem> {
        if idx >= self.inventory.len() {
            return None;
        }
        if self.inventory[idx].qty > 1 {
            self.inventory[idx].qty = self.inventory[idx].qty.saturating_sub(1);
            let mut one = self.inventory[idx].clone();
            one.qty = 1;
            Some(one)
        } else {
            Some(self.inventory.remove(idx))
        }
    }

    fn add_item_to_inventory(&mut self, mut item: InventoryItem) -> bool {
        item.qty = item.qty.max(1);
        if Self::can_stack(&item) {
            let mut remaining = item.qty;
            for existing in self
                .inventory
                .iter_mut()
                .filter(|it| Self::can_stack(it) && it.kind == item.kind)
            {
                if remaining == 0 {
                    break;
                }
                let space = MAX_STACK_QTY.saturating_sub(existing.qty);
                if space == 0 {
                    continue;
                }
                let add = space.min(remaining);
                existing.qty = existing.qty.saturating_add(add);
                remaining = remaining.saturating_sub(add);
            }
            while remaining > 0 {
                if self.inventory_full() {
                    return false;
                }
                let add = remaining.min(MAX_STACK_QTY);
                self.inventory.push(InventoryItem {
                    kind: item.kind,
                    custom_name: None,
                    qty: add,
                });
                remaining = remaining.saturating_sub(add);
            }
            return true;
        }
        item.qty = 1;
        if self.inventory_full() {
            return false;
        }
        self.inventory.push(item);
        true
    }

    pub(crate) fn add_item_kind_to_inventory(&mut self, kind: Item) -> bool {
        self.add_item_to_inventory(InventoryItem {
            kind,
            custom_name: None,
            qty: 1,
        })
    }

    fn inventory_full(&self) -> bool {
        self.inventory.len() >= crate::MAX_INVENTORY
    }

    pub(crate) fn place_ground_item_near_player(&mut self, kind: Item) -> bool {
        self.place_ground_item_near(self.player.x, self.player.y, kind)
    }

    fn place_ground_item_near(&mut self, origin_x: i32, origin_y: i32, kind: Item) -> bool {
        let offsets = [
            (0, 0),
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];
        for (dx, dy) in offsets {
            let x = origin_x + dx;
            let y = origin_y + dy;
            if self.item_at(x, y).is_none() {
                self.ground_items.insert((x, y), kind);
                return true;
            }
        }
        false
    }

    pub(crate) fn stash_or_drop_item(&mut self, item: InventoryItem) {
        if self.add_item_to_inventory(item.clone()) {
            return;
        }
        if self.place_ground_item_near_player(item.kind) {
            self.push_log(trf(
                "game.drop_full",
                &[("item", item.display_name())],
            ));
        } else {
            self.push_log(trf("game.lost_item", &[("item", item.display_name())]));
        }
    }

    fn pick_up_item_at_player(&mut self) {
        let key = (self.player.x, self.player.y);
        let picked = self.ground_items.get(&key).copied();
        if let Some(item) = picked {
            if self.inventory_full() {
                self.push_log(tr("game.inv_full"));
                return;
            }
            self.ground_items.remove(&key);
            let _ = self.add_item_kind_to_inventory(item);
            self.stat_items_picked = self.stat_items_picked.saturating_add(1);
            self.push_log(trf(
                "game.picked",
                &[
                    ("item", crate::localized_item_name(item)),
                    ("count", self.inventory.len().to_string()),
                    ("max", crate::MAX_INVENTORY.to_string()),
                ],
            ));
        }
    }

    pub(crate) fn use_inventory_item(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            self.push_log(tr("game.no_usable"));
            return false;
        }
        let kind = self.inventory[idx].kind;
        match kind {
            Item::Potion => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let before = self.player_hp;
                self.player_hp = (self.player_hp + crate::POTION_HEAL).min(self.player_max_hp);
                let healed = self.player_hp - before;
                if healed > 0 {
                    self.push_log(trf(
                        "game.used_heal",
                        &[
                            ("item", item.display_name()),
                            ("heal", healed.to_string()),
                            ("hp", self.player_hp.to_string()),
                            ("max", self.player_max_hp.to_string()),
                        ],
                    ));
                } else {
                    self.push_log(trf(
                        "game.used_no_heal",
                        &[("item", item.display_name())],
                    ));
                }
                true
            }
            Item::Herb => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let before = self.player_hp;
                self.player_hp = (self.player_hp + 3).min(self.player_max_hp);
                let healed = self.player_hp - before;
                if healed > 0 {
                    self.push_log(trf(
                        "game.used_heal",
                        &[
                            ("item", item.display_name()),
                            ("heal", healed.to_string()),
                            ("hp", self.player_hp.to_string()),
                            ("max", self.player_max_hp.to_string()),
                        ],
                    ));
                } else {
                    self.push_log(trf(
                        "game.used_no_heal",
                        &[("item", item.display_name())],
                    ));
                }
                true
            }
            Item::Elixir => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let before = self.player_hp;
                self.player_hp = (self.player_hp + 12).min(self.player_max_hp);
                let healed = self.player_hp - before;
                if healed > 0 {
                    self.push_log(trf(
                        "game.used_heal",
                        &[
                            ("item", item.display_name()),
                            ("heal", healed.to_string()),
                            ("hp", self.player_hp.to_string()),
                            ("max", self.player_max_hp.to_string()),
                        ],
                    ));
                } else {
                    self.push_log(trf(
                        "game.used_no_heal",
                        &[("item", item.display_name())],
                    ));
                }
                true
            }
            Item::Food => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let before = self.player_hunger;
                self.player_hunger =
                    (self.player_hunger + FOOD_HUNGER_RESTORE).min(self.player_max_hunger);
                let restored = self.player_hunger - before;
                if restored > 0 {
                    self.push_log(trf(
                        "game.used_hunger",
                        &[
                            ("item", item.display_name()),
                            ("v", restored.to_string()),
                            ("cur", self.player_hunger.to_string()),
                            ("max", self.player_max_hunger.to_string()),
                        ],
                    ));
                } else {
                    self.push_log(trf(
                        "game.used_no_hunger",
                        &[("item", item.display_name())],
                    ));
                }
                true
            }
            Item::Bread => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let before = self.player_hunger;
                self.player_hunger =
                    (self.player_hunger + BREAD_HUNGER_RESTORE).min(self.player_max_hunger);
                let restored = self.player_hunger - before;
                if restored > 0 {
                    self.push_log(trf(
                        "game.used_hunger",
                        &[
                            ("item", item.display_name()),
                            ("v", restored.to_string()),
                            ("cur", self.player_hunger.to_string()),
                            ("max", self.player_max_hunger.to_string()),
                        ],
                    ));
                } else {
                    self.push_log(trf(
                        "game.used_no_hunger",
                        &[("item", item.display_name())],
                    ));
                }
                true
            }
            Item::Torch => {
                let p = (self.player.x, self.player.y);
                if self.torches.contains(&p) {
                    self.push_log(tr("game.torch_already"));
                    return false;
                }
                let Some(_item) = self.take_inventory_one(idx) else {
                    return false;
                };
                self.torches.insert(p);
                self.push_log(tr("game.torch_placed"));
                true
            }
            Item::FlameScroll => {
                if !self.try_spend_mp(FLAME_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_flame_scroll();
                true
            }
            Item::BlinkScroll => {
                if !self.try_spend_mp(BLINK_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_blink_scroll();
                true
            }
            Item::NovaScroll => {
                if !self.try_spend_mp(NOVA_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_nova_scroll();
                true
            }
            Item::StoneAxe | Item::IronSword | Item::IronPickaxe => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let equipped_name = item.display_name();
                let old = self.equipped_sword.replace(item);
                if let Some(prev) = old {
                    self.stash_or_drop_item(prev);
                    self.push_log(trf(
                        "game.equipped_item_swap",
                        &[("item", equipped_name)],
                    ));
                } else {
                    self.push_log(trf("game.equipped_item", &[("item", equipped_name)]));
                }
                true
            }
            Item::WoodenShield => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let equipped_name = item.display_name();
                let old = self.equipped_shield.replace(item);
                if let Some(prev) = old {
                    self.stash_or_drop_item(prev);
                    self.push_log(trf(
                        "game.equipped_slot_swap",
                        &[("item", equipped_name), ("slot", tr("status.slot.shield").to_string())],
                    ));
                } else {
                    self.push_log(trf(
                        "game.equipped_slot",
                        &[("item", equipped_name), ("slot", tr("status.slot.shield").to_string())],
                    ));
                }
                true
            }
            Item::LuckyCharm => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let equipped_name = item.display_name();
                let old = self.equipped_accessory.replace(item);
                if let Some(prev) = old {
                    self.stash_or_drop_item(prev);
                    self.push_log(trf(
                        "game.equipped_slot_swap",
                        &[
                            ("item", equipped_name),
                            ("slot", tr("status.slot.accessory").to_string()),
                        ],
                    ));
                } else {
                    self.push_log(trf(
                        "game.equipped_slot",
                        &[
                            ("item", equipped_name),
                            ("slot", tr("status.slot.accessory").to_string()),
                        ],
                    ));
                }
                true
            }
            Item::Wood
            | Item::Stone
            | Item::StringFiber
            | Item::IronIngot
            | Item::Hide => {
                self.push_log(tr("game.cannot_use_direct"));
                false
            }
        }
    }

    fn try_spend_mp(&mut self, cost: i32) -> bool {
        if self.player_mp < cost {
            self.push_log(trf(
                "game.no_mp",
                &[
                    ("need", cost.to_string()),
                    ("mp", self.player_mp.max(0).to_string()),
                ],
            ));
            return false;
        }
        self.player_mp = self.player_mp.saturating_sub(cost);
        true
    }

    fn cast_flame_scroll(&mut self) -> bool {
        let (dx, dy) = self.facing.delta();
        let mut max_reach_step: i32 = 0;
        for step in 1..=6 {
            let tx = self.player.x + dx * step;
            let ty = self.player.y + dy * step;
            if !self.tile(tx, ty).walkable() {
                break;
            }
            max_reach_step = step;
            if let Some(i) = self
                .enemies
                .iter()
                .position(|e| e.pos.x == tx && e.pos.y == ty)
            {
                self.push_flame_line_effect(self.player, self.facing, step as u8, 0, "player");
                let enemy_name = crate::localized_creature_name(&self.enemies[i].creature_id);
                let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
                let damage = calc_damage(self.player_attack_power() + 4, enemy_def);
                self.stat_damage_dealt = self.stat_damage_dealt.saturating_add(damage as u32);
                self.enemies[i].hp -= damage;
                if Self::is_traveler_id(&self.enemies[i].creature_id) {
                    self.enemies[i].flee_from_player = true;
                }
                if self.enemies[i].hp <= 0 {
                    self.stat_enemies_defeated = self.stat_enemies_defeated.saturating_add(1);
                    let dead = self.enemies.remove(i);
                    self.blood_stains.insert((dead.pos.x, dead.pos.y));
                    self.push_death_cry(&dead.creature_id);
                    self.push_log(trf("game.you_defeated", &[("enemy", enemy_name)]));
                    if Self::is_traveler_id(&dead.creature_id) {
                        self.maybe_drop_traveler_bread(dead.pos);
                    }
                } else {
                    self.push_log(trf(
                        "game.you_hit_enemy",
                        &[("enemy", enemy_name), ("damage", damage.to_string())],
                    ));
                }
                return true;
            }
        }
        let length = if max_reach_step > 0 {
            max_reach_step as u8
        } else {
            1
        };
        self.push_flame_line_effect(self.player, self.facing, length, 0, "player");
        self.push_log(tr("game.no_target"));
        true
    }

    fn cast_blink_scroll(&mut self) -> bool {
        let (fx, fy) = self.facing.delta();
        let mut tx = self.player.x;
        let mut ty = self.player.y;
        for _ in 0..4 {
            let nx = tx + fx;
            let ny = ty + fy;
            if !self.tile(nx, ny).walkable() || self.has_enemy_at(nx, ny) {
                break;
            }
            tx = nx;
            ty = ny;
        }
        if (tx, ty) == (self.player.x, self.player.y) {
            self.push_log(tr("game.no_target"));
            return false;
        }
        self.player = Pos { x: tx, y: ty };
        self.pick_up_item_at_player();
        self.push_log(trf(
            "game.blink_to",
            &[("x", tx.to_string()), ("y", ty.to_string())],
        ));
        true
    }

    fn cast_nova_scroll(&mut self) -> bool {
        const NOVA_RADIUS: i32 = 4;
        let radius2 = NOVA_RADIUS * NOVA_RADIUS;
        self.push_visual_effect("nova_burst", self.player, self.facing, "player", 0);

        let mut target_ids: Vec<u64> = self
            .enemies
            .iter()
            .filter_map(|e| {
                let dx = e.pos.x - self.player.x;
                let dy = e.pos.y - self.player.y;
                if dx * dx + dy * dy <= radius2 {
                    Some(e.id)
                } else {
                    None
                }
            })
            .collect();
        target_ids.sort_unstable();

        if target_ids.is_empty() {
            self.push_log(tr("game.no_target"));
            return true;
        }

        for enemy_id in target_ids {
            let Some(i) = self.enemies.iter().position(|e| e.id == enemy_id) else {
                continue;
            };
            let enemy_pos = self.enemies[i].pos;
            self.push_attack_effect(self.player, enemy_pos, 0);
            let enemy_name = crate::localized_creature_name(&self.enemies[i].creature_id);
            let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
            let damage = calc_damage(self.player_attack_power() + 2, enemy_def);
            self.stat_damage_dealt = self.stat_damage_dealt.saturating_add(damage as u32);
            self.enemies[i].hp -= damage;
            if Self::is_traveler_id(&self.enemies[i].creature_id) {
                self.enemies[i].flee_from_player = true;
            }
            if self.enemies[i].hp <= 0 {
                self.stat_enemies_defeated = self.stat_enemies_defeated.saturating_add(1);
                let dead = self.enemies.remove(i);
                self.blood_stains.insert((dead.pos.x, dead.pos.y));
                self.push_death_cry(&dead.creature_id);
                self.push_log(trf("game.you_defeated", &[("enemy", enemy_name)]));
                if Self::is_traveler_id(&dead.creature_id) {
                    self.maybe_drop_traveler_bread(dead.pos);
                }
            } else {
                self.push_log(trf(
                    "game.you_hit_enemy",
                    &[("enemy", enemy_name), ("damage", damage.to_string())],
                ));
            }
        }
        true
    }

    fn maybe_drop_traveler_bread(&mut self, pos: Pos) {
        if self.rand_u32() % 100 < 40 {
            let _ = self.place_ground_item_near(pos.x, pos.y, Item::Bread);
            self.push_log(trf(
                "game.enemy_drop_item",
                &[("item", crate::localized_item_name(Item::Bread))],
            ));
        }
    }

    pub(crate) fn drop_inventory_item(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            return false;
        }
        let key = (self.player.x, self.player.y);
        if self.ground_items.contains_key(&key) {
            self.push_log(tr("game.cannot_drop_here"));
            return false;
        }
        let Some(item) = self.take_inventory_one(idx) else {
            return false;
        };
        self.ground_items.insert(key, item.kind);
        self.push_log(trf("game.dropped", &[("item", item.display_name())]));
        true
    }

    pub(crate) fn throw_inventory_item(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            return false;
        }
        let Some(item) = self.take_inventory_one(idx) else {
            return false;
        };
        let (fx, fy) = self.facing.delta();
        let mut tx = self.player.x;
        let mut ty = self.player.y;
        for _ in 0..3 {
            let nx = tx + fx;
            let ny = ty + fy;
            if !self.tile(nx, ny).walkable() || self.has_enemy_at(nx, ny) {
                break;
            }
            tx = nx;
            ty = ny;
        }
        if (tx, ty) == (self.player.x, self.player.y) {
            self.push_log(trf("game.throw_feet", &[("item", item.display_name())]));
        } else {
            self.push_log(trf(
                "game.throw_to",
                &[("item", item.display_name())],
            ));
        }
        if !self.place_ground_item_near(tx, ty, item.kind) {
            self.push_log(trf("game.lost_item", &[("item", item.display_name())]));
        }
        true
    }

    pub(crate) fn rename_inventory_item(&mut self, idx: usize, new_name: String) -> bool {
        if idx >= self.inventory.len() {
            return false;
        }
        let trimmed = new_name.trim().to_string();
        if trimmed.is_empty() {
            self.inventory[idx].custom_name = None;
            self.push_log(tr("game.rename_reset"));
        } else {
            self.inventory[idx].custom_name = Some(trimmed.clone());
            self.push_log(trf("game.rename_to", &[("name", trimmed)]));
        }
        true
    }

    fn consume_turn(&mut self) {
        self.tick_enemies();
        self.turn = self.turn.saturating_add(1);
        if self.turn.is_multiple_of(3) {
            self.player_hunger = self.player_hunger.saturating_sub(1).max(0);
        }
        self.tick_dark_spawn();
        if self.player_hp > 0
            && self.player_hp < self.player_max_hp
            && self.turn.is_multiple_of(crate::TURN_REGEN_INTERVAL)
        {
            self.player_hp += 1;
        }
    }

    fn gain_exp(&mut self, amount: u32) {
        if amount == 0 {
            return;
        }
        self.exp = self.exp.saturating_add(amount);
        self.stat_total_exp = self.stat_total_exp.saturating_add(amount);
        self.push_log(trf("game.gain_exp", &[("exp", amount.to_string())]));
        while self.exp >= self.next_exp {
            self.exp -= self.next_exp;
            self.level = self.level.saturating_add(1);
            self.next_exp = exp_needed_for_level(self.level);
            self.player_max_hp = self.player_max_hp.saturating_add(3);
            self.player_max_mp = self.player_max_mp.saturating_add(LEVEL_UP_MP_GAIN);
            self.player_mp = (self.player_mp + LEVEL_UP_MP_GAIN).min(self.player_max_mp);
            self.push_log(trf(
                "game.level_up",
                &[
                    ("level", self.level.to_string()),
                    ("hp", self.player_hp.to_string()),
                    ("max", self.player_max_hp.to_string()),
                    ("mp", self.player_mp.to_string()),
                    ("mp_max", self.player_max_mp.to_string()),
                ],
            ));
        }
    }

    pub(crate) fn consume_non_attack_turn(&mut self) {
        self.harvest_state = None;
        self.consume_turn();
    }

    fn is_enemy_passable(&mut self, x: i32, y: i32, occupied: &HashMap<(i32, i32), usize>) -> bool {
        if x == self.player.x && y == self.player.y {
            return false;
        }
        self.tile(x, y).walkable() && !occupied.contains_key(&(x, y))
    }

    fn enemy_move_directions_toward(&self, current: Pos) -> Vec<(i32, i32)> {
        let dx = self.player.x - current.x;
        let dy = self.player.y - current.y;
        let sx = dx.signum();
        let sy = dy.signum();
        let mut dirs: Vec<(i32, i32)> = Vec::with_capacity(8);
        for cand in [
            (sx, sy),
            (sx, 0),
            (0, sy),
            (-sx, sy),
            (sx, -sy),
            (-sx, 0),
            (0, -sy),
            (-sx, -sy),
        ] {
            if cand != (0, 0) && !dirs.contains(&cand) {
                dirs.push(cand);
            }
        }
        for cand in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            if !dirs.contains(&cand) {
                dirs.push(cand);
            }
        }
        dirs
    }

    fn find_enemy_next_step(
        &mut self,
        current: Pos,
        occupied: &HashMap<(i32, i32), usize>,
    ) -> Option<Pos> {
        let dirs = self.enemy_move_directions_toward(current);
        let mut queue: VecDeque<(Pos, Option<Pos>, u8)> = VecDeque::new();
        let mut visited: HashSet<(i32, i32)> = HashSet::new();
        queue.push_back((current, None, 0));
        visited.insert((current.x, current.y));

        while let Some((pos, first_step, depth)) = queue.pop_front() {
            if depth >= ENEMY_PATHFIND_MAX_DEPTH {
                continue;
            }
            for (mx, my) in &dirs {
                let nx = pos.x + mx;
                let ny = pos.y + my;
                if (nx - current.x).abs().max((ny - current.y).abs()) > ENEMY_PATHFIND_MAX_RADIUS {
                    continue;
                }
                if visited.contains(&(nx, ny)) {
                    continue;
                }
                if !self.is_enemy_passable(nx, ny, occupied) {
                    continue;
                }
                visited.insert((nx, ny));
                let step = first_step.unwrap_or(Pos { x: nx, y: ny });
                let chebyshev_to_player = (self.player.x - nx).abs().max((self.player.y - ny).abs());
                if chebyshev_to_player == 1 {
                    return Some(step);
                }
                queue.push_back((Pos { x: nx, y: ny }, Some(step), depth.saturating_add(1)));
            }
        }
        None
    }

    fn choose_spawn_enemy_kind(&mut self, x: i32, y: i32) -> Option<String> {
        let biome = self.world.biome_id_at(x, y);
        let spawnables: Vec<(&str, u32)> = defs()
            .creatures
            .iter()
            .filter_map(|(id, c)| {
                if c.faction == Faction::Hostile
                    && c.spawn_weight > 0
                    && Self::enemy_allowed_on_floor(id, self.floor)
                {
                    let mul = biome_enemy_multiplier(biome, id);
                    let adjusted = c.spawn_weight.saturating_mul(mul).saturating_div(100);
                    if adjusted > 0 {
                        Some((id.as_str(), adjusted))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        if spawnables.is_empty() {
            return None;
        }
        let total_weight: u32 = spawnables.iter().map(|(_, w)| *w).sum();
        let mut r = self.rand_u32() % total_weight.max(1);
        let mut chosen = spawnables[0].0;
        for (id, w) in &spawnables {
            if r < *w {
                chosen = id;
                break;
            }
            r -= *w;
        }
        Some(chosen.to_string())
    }

    fn random_facing(&mut self) -> Facing {
        match self.rand_u32() % 8 {
            0 => Facing::N,
            1 => Facing::NE,
            2 => Facing::E,
            3 => Facing::SE,
            4 => Facing::S,
            5 => Facing::SW,
            6 => Facing::W,
            _ => Facing::NW,
        }
    }

    fn is_traveler_id(id: &str) -> bool {
        id == "traveler"
    }

    fn is_dark_spawn_tile(&self, x: i32, y: i32) -> bool {
        if self.is_lit_by_torch(x, y) {
            return false;
        }
        let dx = x - self.player.x;
        let dy = y - self.player.y;
        let in_vision = dx * dx + dy * dy <= crate::VISION_RADIUS * crate::VISION_RADIUS;
        if !in_vision {
            return true;
        }
        !is_bright_by_facing(self.facing, dx, dy)
    }

    fn enemy_density_score(&self, x: i32, y: i32) -> usize {
        self.enemies
            .iter()
            .filter(|e| {
                (e.pos.x - x).abs().max((e.pos.y - y).abs()) <= DARK_SPAWN_DENSITY_RADIUS
            })
            .count()
    }

    fn tick_dark_spawn(&mut self) {
        if self.turn == 0 || !self.turn.is_multiple_of(DARK_SPAWN_INTERVAL_TURNS) {
            return;
        }

        let mut best: Option<(i32, i32, usize)> = None;
        let mut ties: Vec<(i32, i32)> = Vec::new();
        for dy in -DARK_SPAWN_RADIUS..=DARK_SPAWN_RADIUS {
            for dx in -DARK_SPAWN_RADIUS..=DARK_SPAWN_RADIUS {
                let dist2 = dx * dx + dy * dy;
                if !(DARK_SPAWN_MIN_DIST2..=DARK_SPAWN_RADIUS * DARK_SPAWN_RADIUS).contains(&dist2)
                {
                    continue;
                }
                let x = self.player.x + dx;
                let y = self.player.y + dy;
                if !self.tile(x, y).walkable()
                    || self.has_enemy_at(x, y)
                    || self.item_at(x, y).is_some()
                    || self.has_torch_at(x, y)
                {
                    continue;
                }
                if !self.is_dark_spawn_tile(x, y) {
                    continue;
                }
                let density = self.enemy_density_score(x, y);
                match best {
                    None => {
                        best = Some((x, y, density));
                        ties.clear();
                        ties.push((x, y));
                    }
                    Some((_, _, best_density)) if density < best_density => {
                        best = Some((x, y, density));
                        ties.clear();
                        ties.push((x, y));
                    }
                    Some((_, _, best_density)) if density == best_density => {
                        ties.push((x, y));
                    }
                    _ => {}
                }
            }
        }

        if ties.is_empty() {
            return;
        }
        let pick = ties[(self.rand_u32() as usize) % ties.len()];
        let Some(kind) = self.choose_spawn_enemy_kind(pick.0, pick.1) else {
            return;
        };
        let enemy_id = self.alloc_entity_id();
        let facing = self.random_facing();
        self.enemies.push(Enemy {
            id: enemy_id,
            pos: Pos {
                x: pick.0,
                y: pick.1,
            },
            hp: creature_meta(&kind).hp,
            creature_id: kind,
            facing,
            flee_from_player: false,
        });
    }

    fn spawn_enemies(&mut self, count: usize) {
        let mut spawned = 0usize;
        let mut attempts = 0usize;
        while spawned < count && attempts < count * 800 {
            attempts += 1;
            let dx = self.rand_range_i32(-24, 24);
            let dy = self.rand_range_i32(-24, 24);
            let x = self.player.x + dx;
            let y = self.player.y + dy;
            let dist2 = dx * dx + dy * dy;
            if dist2 < 25 || dist2 > 24 * 24 {
                continue;
            }
            if !self.tile(x, y).walkable() || self.has_enemy_at(x, y) {
                continue;
            }
            let Some(chosen) = self.choose_spawn_enemy_kind(x, y) else {
                return;
            };
            let enemy_id = self.alloc_entity_id();
            let facing = self.random_facing();
            self.enemies.push(Enemy {
                id: enemy_id,
                pos: Pos { x, y },
                hp: creature_meta(&chosen).hp,
                creature_id: chosen,
                facing,
                flee_from_player: false,
            });
            spawned += 1;
        }
    }

    fn spawn_travelers(&mut self, count: usize) {
        let mut spawned = 0usize;
        let mut attempts = 0usize;
        while spawned < count && attempts < count * 800 {
            attempts += 1;
            let dx = self.rand_range_i32(-24, 24);
            let dy = self.rand_range_i32(-24, 24);
            let x = self.player.x + dx;
            let y = self.player.y + dy;
            let dist2 = dx * dx + dy * dy;
            if dist2 < 25 || dist2 > 24 * 24 {
                continue;
            }
            if !self.tile(x, y).walkable() || self.has_enemy_at(x, y) {
                continue;
            }
            let enemy_id = self.alloc_entity_id();
            let facing = self.random_facing();
            self.enemies.push(Enemy {
                id: enemy_id,
                pos: Pos { x, y },
                hp: creature_meta("traveler").hp,
                creature_id: "traveler".to_string(),
                facing,
                flee_from_player: false,
            });
            spawned += 1;
        }
    }

    fn spawn_items(&mut self, count: usize) {
        let mut spawned = 0usize;
        let mut attempts = 0usize;
        while spawned < count && attempts < count * 1000 {
            attempts += 1;
            let dx = self.rand_range_i32(-28, 28);
            let dy = self.rand_range_i32(-28, 28);
            let x = self.player.x + dx;
            let y = self.player.y + dy;
            let dist2 = dx * dx + dy * dy;
            if dist2 < 9 || dist2 > 28 * 28 {
                continue;
            }
            if !self.tile(x, y).walkable() || self.has_enemy_at(x, y) || self.item_at(x, y).is_some() {
                continue;
            }
            let biome = self.world.biome_id_at(x, y);
            let pool = biome_item_pool(biome);
            let total_weight: u32 = pool.iter().map(|(_, w)| *w).sum();
            if total_weight == 0 {
                continue;
            }
            let mut r = self.rand_u32() % total_weight;
            let mut chosen = pool[0].0;
            for (item, w) in pool {
                if r < *w {
                    chosen = *item;
                    break;
                }
                r -= *w;
            }
            self.ground_items.insert((x, y), chosen);
            spawned += 1;
        }
    }

    fn rand_u32(&mut self) -> u32 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        (self.rng_state >> 32) as u32
    }

    fn rand_range_i32(&mut self, min_incl: i32, max_incl: i32) -> i32 {
        let span = (max_incl - min_incl + 1) as u32;
        min_incl + (self.rand_u32() % span) as i32
    }

    fn tick_enemies(&mut self) {
        if self.enemies.is_empty() {
            return;
        }

        let mut occupied: HashMap<(i32, i32), usize> = self
            .enemies
            .iter()
            .enumerate()
            .map(|(i, e)| ((e.pos.x, e.pos.y), i))
            .collect();
        let mut turn_order: Vec<usize> = (0..self.enemies.len()).collect();
        turn_order.sort_by_key(|&i| self.enemies[i].id);
        let mut attack_order: u16 = 0;
        for i in turn_order {
            let current = self.enemies[i].pos;
            occupied.remove(&(current.x, current.y));
            let is_hostile = creature_meta(&self.enemies[i].creature_id).faction == Faction::Hostile;
            let is_traveler = Self::is_traveler_id(&self.enemies[i].creature_id);

            let dx = self.player.x - current.x;
            let dy = self.player.y - current.y;
            let dist2 = dx * dx + dy * dy;
            let chebyshev = dx.abs().max(dy.abs());
            if is_hostile && chebyshev == 1 {
                let enemy_atk = creature_meta(&self.enemies[i].creature_id).attack;
                let damage = calc_damage(enemy_atk, self.player_defense());
                let delay_u16 = (ENEMY_ATTACK_BASE_DELAY_FRAMES as u16)
                    + attack_order * (ENEMY_ATTACK_STAGGER_FRAMES as u16);
                let delay_frames = delay_u16.min(u8::MAX as u16) as u8;
                attack_order = attack_order.saturating_add(1);
                self.push_attack_effect(current, self.player, delay_frames);
                let enemy_name = crate::localized_creature_name(&self.enemies[i].creature_id);
                self.pending_enemy_hits.push(PendingEnemyHit {
                    enemy_name,
                    damage,
                    delay_frames,
                });
                occupied.insert((current.x, current.y), i);
                continue;
            }

            let mut next = current;
            if is_traveler {
                if !self.enemies[i].flee_from_player
                    && !(self.turn + self.enemies[i].id).is_multiple_of(2)
                {
                    occupied.insert((current.x, current.y), i);
                    continue;
                }
                if self.enemies[i].flee_from_player {
                    let mut dirs: Vec<(i32, i32)> = Vec::new();
                    for cand in [
                        (-dx.signum(), -dy.signum()),
                        (-dx.signum(), 0),
                        (0, -dy.signum()),
                        (dx.signum(), -dy.signum()),
                        (-dx.signum(), dy.signum()),
                        (dx.signum(), 0),
                        (0, dy.signum()),
                        (dx.signum(), dy.signum()),
                    ] {
                        if cand != (0, 0) && !dirs.contains(&cand) {
                            dirs.push(cand);
                        }
                    }
                    for cand in [
                        (-1, -1),
                        (0, -1),
                        (1, -1),
                        (-1, 0),
                        (1, 0),
                        (-1, 1),
                        (0, 1),
                        (1, 1),
                    ] {
                        if !dirs.contains(&cand) {
                            dirs.push(cand);
                        }
                    }
                    for (mx, my) in dirs {
                        let nx = current.x + mx;
                        let ny = current.y + my;
                        if self.is_enemy_passable(nx, ny, &occupied) {
                            next = Pos { x: nx, y: ny };
                            if let Some(facing) = Facing::from_delta(mx, my) {
                                self.enemies[i].facing = facing;
                            }
                            break;
                        }
                    }
                } else {
                    let (mx, my) = self.enemies[i].facing.delta();
                    let nx = current.x + mx;
                    let ny = current.y + my;
                    if self.is_enemy_passable(nx, ny, &occupied) {
                        next = Pos { x: nx, y: ny };
                    } else {
                        self.enemies[i].facing = rotate_facing_clockwise(self.enemies[i].facing);
                    }
                }
            } else {
                // Hostiles outside player vision patrol like travelers:
                // go straight, rotate clockwise on dead end.
                if dist2 > crate::VISION_RADIUS * crate::VISION_RADIUS {
                    let (mx, my) = self.enemies[i].facing.delta();
                    let nx = current.x + mx;
                    let ny = current.y + my;
                    if self.is_enemy_passable(nx, ny, &occupied) {
                        next = Pos { x: nx, y: ny };
                    } else {
                        self.enemies[i].facing = rotate_facing_clockwise(self.enemies[i].facing);
                    }
                    self.enemies[i].pos = next;
                    occupied.insert((next.x, next.y), i);
                    continue;
                }
                if dist2 <= ((ENEMY_PATHFIND_MAX_RADIUS as i32) * (ENEMY_PATHFIND_MAX_RADIUS as i32)) {
                    if let Some(step) = self.find_enemy_next_step(current, &occupied) {
                        next = step;
                    }
                } else {
                    let dirs = self.enemy_move_directions_toward(current);
                    for (mx, my) in dirs {
                        let nx = current.x + mx;
                        let ny = current.y + my;
                        if self.is_enemy_passable(nx, ny, &occupied) {
                            next = Pos { x: nx, y: ny };
                            break;
                        }
                    }
                }
            }

            self.enemies[i].pos = next;
            occupied.insert((next.x, next.y), i);
        }
    }

    pub(crate) fn player_attack_power(&self) -> i32 {
        creature_meta("player").attack
            + self.equipped_attack_bonus()
            + (self.level.saturating_sub(1) as i32)
    }

    pub(crate) fn player_defense(&self) -> i32 {
        creature_meta("player").defense
            + ((self.level.saturating_sub(1) as i32) / 2)
            + self.equipped_defense_bonus()
    }

    pub(crate) fn generated_chunks(&self) -> usize {
        self.world.chunks.len()
    }

    fn enemy_allowed_on_floor(id: &str, floor: u32) -> bool {
        match id {
            "slime_brute" => floor >= 3,
            "wolf_alpha" => floor >= 4,
            "bat_night" => floor >= 5,
            "golem_elder" => floor >= 6,
            _ => true,
        }
    }

    pub(crate) fn current_biome_name(&mut self) -> &'static str {
        self.world.biome_name_at(self.player.x, self.player.y)
    }

    fn equipped_attack_bonus(&self) -> i32 {
        let sword = match self.equipped_sword.as_ref().map(|t| t.kind) {
            Some(Item::StoneAxe) => 2,
            Some(Item::IronPickaxe) => 3,
            Some(Item::IronSword) => 4,
            _ => 0,
        };
        let accessory = match self.equipped_accessory.as_ref().map(|t| t.kind) {
            Some(Item::LuckyCharm) => 1,
            _ => 0,
        };
        sword + accessory
    }

    fn equipped_defense_bonus(&self) -> i32 {
        let shield = match self.equipped_shield.as_ref().map(|t| t.kind) {
            Some(Item::WoodenShield) => 3,
            _ => 0,
        };
        let accessory = match self.equipped_accessory.as_ref().map(|t| t.kind) {
            Some(Item::LuckyCharm) => 1,
            _ => 0,
        };
        shield + accessory
    }

    fn push_attack_effect(&mut self, from: Pos, to: Pos, delay_frames: u8) {
        self.attack_effects.push(AttackEffect {
            from,
            to,
            delay_frames,
            ttl_frames: 1,
        });
        let facing =
            Facing::from_delta(to.x - from.x, to.y - from.y).unwrap_or(Facing::E);
        let style = if from.x == self.player.x && from.y == self.player.y {
            "player"
        } else {
            "enemy"
        };
        self.push_visual_effect("attack_normal", to, facing, style, delay_frames);
    }

    fn push_visual_effect(
        &mut self,
        effect_id: &str,
        origin: Pos,
        facing: Facing,
        style_key: &str,
        delay_frames: u8,
    ) {
        self.visual_effects.push(ActiveVisualEffect {
            effect_id: effect_id.to_string(),
            origin,
            facing,
            style_key: style_key.to_string(),
            delay_frames,
            frame_index: 0,
            frame_tick: 0,
        });
    }

    fn push_flame_line_effect(
        &mut self,
        origin: Pos,
        facing: Facing,
        length: u8,
        base_delay: u8,
        style_key: &str,
    ) {
        let (dx, dy) = facing.delta();
        for step in 1..=length.max(1) {
            let pos = Pos {
                x: origin.x + dx * step as i32,
                y: origin.y + dy * step as i32,
            };
            let delay = base_delay.saturating_add((step - 1).saturating_mul(2));
            self.push_visual_effect("flame_breath", pos, facing, style_key, delay);
        }
    }

    pub(crate) fn active_effect_cells(&self) -> Vec<EffectCell> {
        let catalog = effect_catalog();
        let mut cells: Vec<EffectCell> = Vec::new();
        for fx in &self.visual_effects {
            if fx.delay_frames > 0 {
                continue;
            }
            let Some(def) = catalog.effects.get(&fx.effect_id) else {
                continue;
            };
            if def.frames.is_empty() {
                continue;
            }
            let idx = (fx.frame_index as usize).min(def.frames.len() - 1);
            let base = &def.frames[idx];
            let oriented = if def.auto_rotate {
                rotate_frame(base, def.size, def.base_direction, fx.facing)
            } else {
                base.clone()
            };
            let style = def
                .style_presets
                .get(&fx.style_key)
                .copied()
                .unwrap_or(EffectStyle {
                    color: Color::Yellow,
                    bold: true,
                });
            let center = (def.size / 2) as i32;
            for y in 0..def.size {
                for x in 0..def.size {
                    let ch = oriented[y * def.size + x];
                    if ch == '\0' || ch == def.transparent_char {
                        continue;
                    }
                    let ch = oriented_tip_glyph(ch, fx.facing);
                    cells.push(EffectCell {
                        x: fx.origin.x + x as i32 - center,
                        y: fx.origin.y + y as i32 - center,
                        glyph: ch,
                        color: style.color,
                        bold: style.bold,
                    });
                }
            }
        }
        cells
    }

    pub(crate) fn advance_effects(&mut self) {
        for fx in &mut self.attack_effects {
            if fx.delay_frames > 0 {
                fx.delay_frames = fx.delay_frames.saturating_sub(1);
                continue;
            }
            fx.ttl_frames = fx.ttl_frames.saturating_sub(1);
        }
        self.attack_effects
            .retain(|fx| fx.delay_frames > 0 || fx.ttl_frames > 0);

        let catalog = effect_catalog();
        let mut next_visual: Vec<ActiveVisualEffect> = Vec::with_capacity(self.visual_effects.len());
        for mut fx in std::mem::take(&mut self.visual_effects) {
            if fx.delay_frames > 0 {
                fx.delay_frames = fx.delay_frames.saturating_sub(1);
                next_visual.push(fx);
                continue;
            }
            let Some(def) = catalog.effects.get(&fx.effect_id) else {
                continue;
            };
            if def.frames.is_empty() {
                continue;
            }
            fx.frame_tick = fx.frame_tick.saturating_add(1);
            if fx.frame_tick >= def.frame_duration {
                fx.frame_tick = 0;
                fx.frame_index = fx.frame_index.saturating_add(1);
            }
            if (fx.frame_index as usize) < def.frames.len() {
                next_visual.push(fx);
            }
        }
        self.visual_effects = next_visual;

        let mut any_hit_applied = false;
        let mut next_hits: Vec<PendingEnemyHit> = Vec::with_capacity(self.pending_enemy_hits.len());
        let pending_hits = std::mem::take(&mut self.pending_enemy_hits);
        for mut hit in pending_hits {
            if hit.delay_frames > 0 {
                hit.delay_frames = hit.delay_frames.saturating_sub(1);
                next_hits.push(hit);
                continue;
            }
            if !self.invincible {
                self.player_hp -= hit.damage;
                self.stat_damage_taken = self.stat_damage_taken.saturating_add(hit.damage as u32);
                self.push_log(trf(
                    "game.enemy_hit_you",
                    &[("enemy", hit.enemy_name), ("damage", hit.damage.to_string())],
                ));
                any_hit_applied = true;
            }
        }
        self.pending_enemy_hits = next_hits;
        if any_hit_applied && self.player_hp <= 0 {
            self.push_log(tr("game.you_slain"));
        }
    }

    pub(crate) fn has_pending_effects(&self) -> bool {
        !self.attack_effects.is_empty()
            || !self.visual_effects.is_empty()
            || !self.pending_enemy_hits.is_empty()
    }

    pub(crate) fn snapshot(&self) -> GameSnapshot {
        let mut chunks: Vec<ChunkState> = self
            .world
            .chunks
            .iter()
            .map(|(&(chunk_x, chunk_y), chunk)| ChunkState {
                chunk_x,
                chunk_y,
                tiles: chunk.tiles.to_vec(),
                biome: chunk.biome.0,
                biome_noise_a: chunk.biome_noise_a,
                biome_noise_b: chunk.biome_noise_b,
            })
            .collect();
        chunks.sort_by_key(|c| (c.chunk_x, c.chunk_y));

        let mut ground_items: Vec<GroundItemState> = self
            .ground_items
            .iter()
            .map(|(&(x, y), &item)| GroundItemState { x, y, item })
            .collect();
        ground_items.sort_by_key(|g| (g.x, g.y));

        GameSnapshot {
            seed: self.world.seed,
            chunks,
            player: PosState {
                x: self.player.x,
                y: self.player.y,
            },
            player_id: self.player_id,
            facing: self.facing,
            player_hp: self.player_hp,
            player_max_hp: self.player_max_hp,
            player_mp: self.player_mp,
            player_max_mp: self.player_max_mp,
            player_hunger: self.player_hunger,
            player_max_hunger: self.player_max_hunger,
            inventory: self.inventory.clone(),
            equipped_sword: self.equipped_sword.clone(),
            equipped_shield: self.equipped_shield.clone(),
            equipped_accessory: self.equipped_accessory.clone(),
            enemies: self
                .enemies
                .iter()
                .map(|e| EnemyState {
                    id: e.id,
                    pos: PosState {
                        x: e.pos.x,
                        y: e.pos.y,
                    },
                    hp: e.hp,
                    creature_id: e.creature_id.clone(),
                    facing: e.facing,
                    flee_from_player: e.flee_from_player,
                })
                .collect(),
            ground_items,
            blood_stains: self
                .blood_stains
                .iter()
                .map(|&(x, y)| PosState { x, y })
                .collect(),
            torches: self
                .torches
                .iter()
                .map(|&(x, y)| TorchState { x, y })
                .collect(),
            harvest_state: self.harvest_state.map(|h| HarvestStateState {
                x: h.target.0,
                y: h.target.1,
                hits: h.hits,
            }),
            rng_state: self.rng_state,
            turn: self.turn,
            floor: self.floor,
            level: self.level,
            exp: self.exp,
            next_exp: self.next_exp,
            next_entity_id: self.next_entity_id,
            stat_enemies_defeated: self.stat_enemies_defeated,
            stat_damage_dealt: self.stat_damage_dealt,
            stat_damage_taken: self.stat_damage_taken,
            stat_items_picked: self.stat_items_picked,
            stat_steps: self.stat_steps,
            stat_total_exp: self.stat_total_exp,
            logs: self.logs.clone(),
        }
    }

    pub(crate) fn from_snapshot(snapshot: GameSnapshot) -> Result<Self, String> {
        let mut chunks = HashMap::new();
        for c in snapshot.chunks {
            if c.tiles.len() != crate::CHUNK_AREA {
                return Err(format!(
                    "invalid chunk tile count at ({}, {})",
                    c.chunk_x, c.chunk_y
                ));
            }
            let tiles: [Tile; crate::CHUNK_AREA] = c
                .tiles
                .try_into()
                .map_err(|_| "failed to restore chunk array".to_string())?;
            chunks.insert(
                (c.chunk_x, c.chunk_y),
                Chunk {
                    tiles,
                    biome: BiomeId::new(c.biome),
                    biome_noise_a: c.biome_noise_a,
                    biome_noise_b: c.biome_noise_b,
                },
            );
        }

        let mut ground_items = HashMap::new();
        for g in snapshot.ground_items {
            ground_items.insert((g.x, g.y), g.item);
        }
        let mut blood_stains = HashSet::new();
        for b in snapshot.blood_stains {
            blood_stains.insert((b.x, b.y));
        }
        let mut torches = HashSet::new();
        for t in snapshot.torches {
            torches.insert((t.x, t.y));
        }

        let mut next_auto_enemy_id = snapshot.player_id.saturating_add(1).max(1);
        for e in &snapshot.enemies {
            if e.id > 0 && e.id >= next_auto_enemy_id {
                next_auto_enemy_id = e.id.saturating_add(1);
            }
        }
        let enemies: Vec<Enemy> = snapshot
            .enemies
            .into_iter()
            .map(|e| {
                let id = if e.id == 0 {
                    let assigned = next_auto_enemy_id;
                    next_auto_enemy_id = next_auto_enemy_id.saturating_add(1);
                    assigned
                } else {
                    e.id
                };
                Enemy {
                    id,
                    pos: Pos {
                        x: e.pos.x,
                        y: e.pos.y,
                    },
                    hp: e.hp,
                    creature_id: e.creature_id,
                    facing: e.facing,
                    flee_from_player: e.flee_from_player,
                }
            })
            .collect();

        Ok(Self {
            world: World {
                seed: snapshot.seed,
                chunks,
            },
            player: Pos {
                x: snapshot.player.x,
                y: snapshot.player.y,
            },
            player_id: snapshot.player_id,
            facing: snapshot.facing,
            player_hp: snapshot.player_hp,
            player_max_hp: snapshot.player_max_hp,
            player_mp: snapshot.player_mp.clamp(0, snapshot.player_max_mp.max(1)),
            player_max_mp: snapshot.player_max_mp.max(1),
            player_hunger: snapshot
                .player_hunger
                .clamp(0, snapshot.player_max_hunger.max(1)),
            player_max_hunger: snapshot.player_max_hunger.max(1),
            inventory: snapshot.inventory,
            equipped_sword: snapshot.equipped_sword,
            equipped_shield: snapshot.equipped_shield,
            equipped_accessory: snapshot.equipped_accessory,
            enemies,
            ground_items,
            blood_stains,
            torches,
            attack_effects: Vec::new(),
            visual_effects: Vec::new(),
            pending_enemy_hits: Vec::new(),
            harvest_state: snapshot.harvest_state.map(|h| HarvestState {
                target: (h.x, h.y),
                hits: h.hits,
            }),
            rng_state: snapshot.rng_state,
            next_entity_id: snapshot.next_entity_id.max(next_auto_enemy_id).max(1),
            turn: snapshot.turn,
            floor: snapshot.floor.max(1),
            level: snapshot.level.max(1),
            exp: snapshot.exp,
            next_exp: snapshot.next_exp.max(1),
            stat_enemies_defeated: snapshot.stat_enemies_defeated,
            stat_damage_dealt: snapshot.stat_damage_dealt,
            stat_damage_taken: snapshot.stat_damage_taken,
            stat_items_picked: snapshot.stat_items_picked,
            stat_steps: snapshot.stat_steps,
            stat_total_exp: snapshot.stat_total_exp,
            logs: snapshot.logs,
            pending_dialogue: None,
            invincible: false,
        })
    }
}

fn calc_damage(attack: i32, defense: i32) -> i32 {
    (attack - defense).max(1)
}

fn destructible_info(tile: Tile) -> Option<(u8, Option<Item>, u8, Tile, &'static str)> {
    let meta = tile_meta(tile);
    let hits = meta.harvest_hits?;
    let replace = meta.harvest_replace?;
    let label = meta.harvest_label.as_deref().unwrap_or(meta.legend.as_str());
    Some((
        hits,
        meta.harvest_drop,
        meta.harvest_drop_chance,
        replace,
        label,
    ))
}
