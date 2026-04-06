use std::collections::{HashMap, HashSet, VecDeque};
use std::f32::consts::FRAC_PI_4;
use std::sync::OnceLock;

use ratatui::prelude::Color;
use serde::{Deserialize, Serialize};

use crate::defs::{Faction, creature_meta, defs, item_meta, tile_meta};
use crate::text::{current_lang, set_lang, tr, trf, trf_map};
use crate::{Facing, InventoryItem, Item, Tile};

const LEVEL_EXP_BASE: u32 = 20;
const LEVEL_EXP_STEP: u32 = 12;
const MAX_STACK_QTY: u16 = 10;
const ENEMY_ATTACK_BASE_DELAY_FRAMES: u8 = 4;
const ENEMY_ATTACK_STAGGER_FRAMES: u8 = 6;
const ENEMY_PATHFIND_MAX_DEPTH: u8 = 24;
const ENEMY_PATHFIND_MAX_RADIUS: i32 = 24;
const PLAYER_BASE_MP: i32 = 10;
const LEVEL_UP_MP_GAIN: i32 = 2;
const FLAME_SCROLL_MP_COST: i32 = 2;
const EMBER_SCROLL_MP_COST: i32 = 3;
const BLINK_SCROLL_MP_COST: i32 = 3;
const BIND_SCROLL_MP_COST: i32 = 3;
const REPULSE_SCROLL_MP_COST: i32 = 4;
const NOVA_SCROLL_MP_COST: i32 = 5;
const FORGE_SCROLL_MP_COST: i32 = 4;
const FORGE_SCROLL_ATK_BONUS_GAIN: i32 = 1;
const FORGE_SCROLL_ATK_BONUS_MAX: i32 = 6;
const PLAYER_BASE_HUNGER: i32 = 100;
const FOOD_HUNGER_RESTORE: i32 = 50;
const BREAD_HUNGER_RESTORE: i32 = 100;
const ITEM_USE_HUNGER_RESTORE: i32 = 5;
const TORCH_LIGHT_RADIUS: i32 = 5;
const DARK_SPAWN_INTERVAL_TURNS: u64 = 6;
const DARK_SPAWN_RADIUS: i32 = 24;
const DARK_SPAWN_MIN_DIST2: i32 = 25;
const DARK_SPAWN_DENSITY_RADIUS: i32 = 6;
const INITIAL_TRAVELER_COUNT: usize = 2;
const STAIRS_PEAK_THRESHOLD: f64 = 0.999;
const STAIRS_PEAK_RADIUS: i32 = 2;
const STAIRS_NO_SPAWN_RADIUS2: i32 = 12 * 12;
const ITEM_PEAK_THRESHOLD: f64 = 0.98;
const ITEM_PEAK_RADIUS: i32 = 2;
const ITEM_NO_SPAWN_RADIUS2: i32 = 8 * 8;
const NOISE_SALT_STAIRS: u64 = 0xA1C5_7A17_5EED_11A2;
const NOISE_SALT_ITEMS_PEAK: u64 = 0x11E7_A5E1_D3AD_B33F;
const NOISE_SALT_ITEMS_PICK: u64 = 0xC0DE_17E5_5EED_0001;
const NOISE_SALT_TABLET_PLACE: u64 = 0x7A8B_11E7_CAFE_0001;
const NOISE_SALT_STRUCTURE_PLACE: u64 = 0x22C1_7001_5EED_9002;

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
    color_override: Option<Color>,
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
    carried_items: Vec<CarriedItem>,
    equipped_weapon: Option<Item>,
    status: StatusState,
    facing: Facing,
    flee_from_player: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct CarriedItem {
    item: Item,
    drop_chance: u8,
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
    burning_turns: u8,
    slowed_turns: u8,
    delay_frames: u8,
    attacker_pos: Pos,
    attacker_agility: i32,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct StatusState {
    #[serde(default)]
    burning_turns: u8,
    #[serde(default)]
    slowed_turns: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BiomeId(u8);

impl BiomeId {
    fn new(v: u8) -> Self {
        Self(v.min(15))
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum MapPattern {
    Perlin,
    RogueRooms,
}

#[derive(Clone, Copy)]
enum TerrainTheme {
    SurfaceRuin,
    BurialVein,
    ResearchShaft,
    LitanyHalls,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StairsMode {
    Normal,
    FacilityLocked,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpecialFacilityMode {
    None,
    Substory,
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
    if val == key { fallback } else { val }
}

fn biome_enemy_multiplier(biome: BiomeId, creature_id: &str) -> u32 {
    let bx = biome.0 % 4;
    let by = biome.0 / 4;
    let edge = bx == 0 || bx == 3 || by == 0 || by == 3;
    match creature_id {
        "melted_husk" | "engorged_husk" | "ash_eater" | "buried_ember" => match (bx, by) {
            (1, 0) | (2, 0) | (1, 1) | (2, 1) => 220,
            (1, 2) | (2, 2) => 140,
            _ => 70,
        },
        "feral_vessel" | "dominion_vessel" | "scavenger_knight" | "seal_hound" | "specimen_guard" => match (bx, by) {
            (2, 1) | (3, 1) | (2, 2) | (3, 2) => 220,
            (1, 1) | (1, 2) => 130,
            _ => 55,
        },
        "choir_mote" | "night_choir" | "prayer_remnant" | "incense_shell" => match (bx, by) {
            (2, 2) | (3, 2) | (2, 3) | (3, 3) => 220,
            (1, 2) | (1, 3) => 125,
            _ => 55,
        },
        "grave_frame" | "cathedral_frame" | "stone_warden" | "tomb_warden" | "coffin_bearer" | "carrier_frame" => {
            if edge {
                190
            } else {
                75
            }
        }
        _ => 100,
    }
}

fn research_spawn_multiplier(score: u32, creature_id: &str) -> u32 {
    if score == 0 {
        return 100;
    }
    match creature_id {
        "archive_scribe" => (100 + score.saturating_mul(40)).min(420),
        "relay_surgeon" => (100 + score.saturating_mul(34)).min(380),
        "carrier_frame" | "specimen_guard" | "flayed_specimen" => {
            (100 + score.saturating_mul(24)).min(300)
        }
        "night_choir" | "cathedral_frame" | "scavenger_knight" | "dominion_vessel" => {
            (100 + score.saturating_mul(14)).min(220)
        }
        "melted_husk" | "feral_vessel" | "choir_mote" | "ash_eater" | "prayer_remnant" => {
            (100i32 - (score as i32 * 9)).max(35) as u32
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
            (Item::ForgeScroll, 1),
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
            (Item::ForgeScroll, 1),
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
            (Item::ForgeScroll, 2),
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
            (Item::ForgeScroll, 1),
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

fn tile_blocks_sight(tile: Tile) -> bool {
    matches!(tile, Tile::Wall | Tile::Rock | Tile::Mountain | Tile::Forest)
}

fn biome_profile(id: BiomeId) -> BiomeProfile {
    let x = (id.0 % 4) as f64;
    let y = (id.0 / 4) as f64;
    let nx = (x - 1.5) / 1.5;
    let ny = (y - 1.5) / 1.5;
    let edge = nx.abs().max(ny.abs());
    let rugged = (nx * 0.65 + ny * 0.35).clamp(-1.0, 1.0);
    let wet = (ny * 0.8 - nx * 0.25).clamp(-1.0, 1.0);
    // Strong per-biome shaping to make floor-band palettes feel visually distinct.
    let (extra_elev, extra_abyss, extra_rock, extra_wall) = match id.0 {
        // tier_1: greener / more open
        2 | 3 | 5 | 6 | 7 => (0.03, -0.08, 0.09, 0.07),
        // tier_2: marsh + mixed plateau
        0 | 1 | 4 | 8 | 9 | 10 => (-0.01, 0.04, 0.03, 0.01),
        // tier_3: crag/depth (more abyss + rocks/walls)
        11 | 12 | 13 | 14 | 15 => (-0.04, 0.10, -0.10, -0.08),
        _ => (0.0, 0.0, 0.0, 0.0),
    };
    BiomeProfile {
        elevation_bias: rugged * 0.06 + wet * 0.02 + extra_elev,
        abyss_shift: (-wet * 0.04 + edge * 0.03 + extra_abyss).clamp(-0.10, 0.16),
        rock_shift: (-rugged * 0.04 - edge * 0.05 + extra_rock).clamp(-0.18, 0.12),
        wall_shift: (-rugged * 0.03 - edge * 0.04 + extra_wall).clamp(-0.14, 0.10),
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
    biome_palette: Vec<u8>,
    map_pattern: MapPattern,
    terrain_theme: TerrainTheme,
    stairs_mode: StairsMode,
    #[allow(dead_code)]
    special_facility_mode: SpecialFacilityMode,
    chunks: HashMap<(i32, i32), Chunk>,
}

impl World {
    fn new(seed: u64, floor: u32) -> Self {
        Self {
            seed,
            biome_palette: crate::world_cfg::biomes_for_floor(floor),
            map_pattern: Self::pattern_for_floor(floor),
            terrain_theme: Self::theme_for_floor(floor),
            stairs_mode: Self::stairs_mode_for_floor(floor),
            special_facility_mode: Self::special_facility_mode_for_floor(floor),
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
        let biome_palette = self.biome_palette.clone();
        let map_pattern = self.map_pattern;
        let terrain_theme = self.terrain_theme;
        let stairs_mode = self.stairs_mode;
        self.chunks
            .entry((chunk_x, chunk_y))
            .or_insert_with(|| {
                Self::generate_chunk(
                    self.seed,
                    map_pattern,
                    terrain_theme,
                    stairs_mode,
                    chunk_x,
                    chunk_y,
                    &biome_palette,
                )
            })
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

    fn remap_biome(raw: u8, palette: &[u8]) -> BiomeId {
        if palette.is_empty() {
            return BiomeId::new(raw);
        }
        let idx = (raw as usize) % palette.len();
        BiomeId::new(palette[idx])
    }

    fn pattern_for_floor(floor: u32) -> MapPattern {
        match crate::world_cfg::map_pattern_for_floor(floor).as_str() {
            "rogue" => MapPattern::RogueRooms,
            _ => MapPattern::Perlin,
        }
    }

    fn theme_for_floor(floor: u32) -> TerrainTheme {
        match crate::world_cfg::terrain_theme_for_floor(floor).as_str() {
            "burial_vein" => TerrainTheme::BurialVein,
            "research_shaft" => TerrainTheme::ResearchShaft,
            "litany_halls" => TerrainTheme::LitanyHalls,
            _ => TerrainTheme::SurfaceRuin,
        }
    }

    fn stairs_mode_for_floor(floor: u32) -> StairsMode {
        match crate::world_cfg::stairs_mode_for_floor(floor).as_str() {
            "facility_locked" => StairsMode::FacilityLocked,
            _ => StairsMode::Normal,
        }
    }

    fn special_facility_mode_for_floor(floor: u32) -> SpecialFacilityMode {
        match crate::world_cfg::special_facility_mode_for_floor(floor).as_str() {
            "substory" => SpecialFacilityMode::Substory,
            _ => SpecialFacilityMode::None,
        }
    }

    fn generate_chunk(
        seed: u64,
        map_pattern: MapPattern,
        terrain_theme: TerrainTheme,
        stairs_mode: StairsMode,
        chunk_x: i32,
        chunk_y: i32,
        biome_palette: &[u8],
    ) -> Chunk {
        let biome_noise_a_gen = crate::noise::Perlin2D::new(seed ^ 0x9E37_79B9_AA55_AA55);
        let biome_noise_b_gen = crate::noise::Perlin2D::new(seed ^ 0xC2B2_AE35_1234_5678);
        let biome_a = biome_noise_a_gen.noise01(chunk_x as f64 * 0.19, chunk_y as f64 * 0.19);
        let biome_b =
            biome_noise_b_gen.noise01(chunk_x as f64 * 0.19 + 111.7, chunk_y as f64 * 0.19 - 77.3);
        let bx = Self::quantize_biome_axis(biome_a);
        let by = Self::quantize_biome_axis(biome_b);
        let biome = Self::remap_biome(by * 4 + bx, biome_palette);
        match map_pattern {
            MapPattern::Perlin => {
                Self::generate_chunk_perlin(
                    seed,
                    terrain_theme,
                    stairs_mode,
                    chunk_x,
                    chunk_y,
                    biome,
                    biome_a,
                    biome_b,
                )
            }
            MapPattern::RogueRooms => {
                Self::generate_chunk_rogue(
                    seed,
                    terrain_theme,
                    stairs_mode,
                    chunk_x,
                    chunk_y,
                    biome,
                    biome_a,
                    biome_b,
                )
            }
        }
    }

    fn generate_chunk_perlin(
        seed: u64,
        terrain_theme: TerrainTheme,
        stairs_mode: StairsMode,
        chunk_x: i32,
        chunk_y: i32,
        biome: BiomeId,
        biome_a: f64,
        biome_b: f64,
    ) -> Chunk {
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
                let (abyss_add, rock_add, wall_add) = match terrain_theme {
                    TerrainTheme::SurfaceRuin => (0.0, 0.0, 0.0),
                    TerrainTheme::BurialVein => (-0.06, -0.04, -0.02),
                    TerrainTheme::ResearchShaft => (0.02, 0.04, 0.06),
                    TerrainTheme::LitanyHalls => (-0.02, 0.02, 0.08),
                };
                let abyss_threshold =
                    (crate::ABYSS_THRESHOLD + profile.abyss_shift + abyss_add).clamp(0.03, 0.48);
                let rock_threshold =
                    (crate::ROCK_THRESHOLD + profile.rock_shift + rock_add).clamp(0.40, 0.90);
                let wall_threshold = (crate::WALL_THRESHOLD + profile.wall_shift + wall_add)
                    .clamp(rock_threshold + 0.02, 0.96);

                let tile = if h <= abyss_threshold {
                    Tile::Abyss
                } else if h >= wall_threshold {
                    Tile::Wall
                } else if h >= rock_threshold {
                    Tile::Rock
                } else {
                    match terrain_theme {
                        TerrainTheme::SurfaceRuin => Tile::from_height(h),
                        TerrainTheme::BurialVein => {
                            if h < 0.58 { Tile::Sand } else if h < 0.78 { Tile::Grass } else { Tile::Forest }
                        }
                        TerrainTheme::ResearchShaft => {
                            if h < 0.54 { Tile::Sand } else if h < 0.70 { Tile::Grass } else { Tile::Rock }
                        }
                        TerrainTheme::LitanyHalls => {
                            if h < 0.50 { Tile::Grass } else if h < 0.76 { Tile::Sand } else { Tile::Forest }
                        }
                    }
                };
                let tile = if stairs_mode == StairsMode::Normal
                    && tile.walkable()
                    && Self::is_stairs_peak(seed, world_x, world_y)
                {
                    Tile::StairsDown
                } else {
                    tile
                };

                chunk.set(local_x, local_y, tile);
            }
        }

        chunk
    }

    fn generate_chunk_rogue(
        seed: u64,
        terrain_theme: TerrainTheme,
        stairs_mode: StairsMode,
        chunk_x: i32,
        chunk_y: i32,
        biome: BiomeId,
        biome_a: f64,
        biome_b: f64,
    ) -> Chunk {
        let mut chunk = Chunk::new(Tile::Wall, biome, biome_a, biome_b);
        let base_x = chunk_x * crate::CHUNK_SIZE as i32;
        let base_y = chunk_y * crate::CHUNK_SIZE as i32;

        #[derive(Clone, Copy)]
        struct Room {
            x: usize,
            y: usize,
            w: usize,
            h: usize,
        }
        impl Room {
            fn center(self) -> (usize, usize) {
                (self.x + self.w / 2, self.y + self.h / 2)
            }
            fn intersects(self, other: Room) -> bool {
                let l1 = self.x.saturating_sub(1);
                let t1 = self.y.saturating_sub(1);
                let r1 = self.x + self.w;
                let b1 = self.y + self.h;
                let l2 = other.x.saturating_sub(1);
                let t2 = other.y.saturating_sub(1);
                let r2 = other.x + other.w;
                let b2 = other.y + other.h;
                l1 < r2 && l2 < r1 && t1 < b2 && t2 < b1
            }
        }

        let mut rooms: Vec<Room> = Vec::new();
        let target_rooms =
            4 + (deterministic_hash64(seed, 0x44AA_1020_7060_F00D, chunk_x, chunk_y) % 4) as usize;
        let mut tries = 0usize;
        while rooms.len() < target_rooms && tries < 36 {
            tries += 1;
            let salt = 0x1234_5000_0000_0000u64.wrapping_add(tries as u64);
            let w = 3 + (deterministic_hash64(seed, salt ^ 0x11, chunk_x, chunk_y) % 5) as usize;
            let h = 3 + (deterministic_hash64(seed, salt ^ 0x22, chunk_x, chunk_y) % 4) as usize;
            let max_x = crate::CHUNK_SIZE.saturating_sub(w + 2);
            let max_y = crate::CHUNK_SIZE.saturating_sub(h + 2);
            if max_x == 0 || max_y == 0 {
                continue;
            }
            let x = 1 + (deterministic_hash64(seed, salt ^ 0x33, chunk_x, chunk_y) as usize % max_x);
            let y = 1 + (deterministic_hash64(seed, salt ^ 0x44, chunk_x, chunk_y) as usize % max_y);
            let candidate = Room { x, y, w, h };
            if rooms.iter().any(|r| candidate.intersects(*r)) {
                continue;
            }
            rooms.push(candidate);
        }

        if rooms.is_empty() {
            rooms.push(Room {
                x: 5,
                y: 5,
                w: 6,
                h: 6,
            });
        }

        let floor_tile = |wx: i32, wy: i32| -> Tile {
            let roll = deterministic_noise01(seed, 0xDEAD_BEEF_5500_0042, wx, wy);
            match terrain_theme {
                TerrainTheme::SurfaceRuin => match biome.0 {
                    2 | 3 | 5 | 6 | 7 => {
                        if roll < 0.70 { Tile::Grass } else { Tile::Sand }
                    }
                    11 | 12 | 13 | 14 | 15 => {
                        if roll < 0.74 { Tile::Sand } else { Tile::Grass }
                    }
                    _ => {
                        if roll < 0.46 { Tile::Sand } else { Tile::Grass }
                    }
                },
                TerrainTheme::BurialVein => {
                    if roll < 0.66 { Tile::Sand } else { Tile::Grass }
                }
                TerrainTheme::ResearchShaft => {
                    if roll < 0.56 { Tile::Sand } else { Tile::Grass }
                }
                TerrainTheme::LitanyHalls => {
                    if roll < 0.42 { Tile::Grass } else { Tile::Sand }
                }
            }
        };

        let carve = |lx: usize, ly: usize, chunk: &mut Chunk| {
            if lx >= crate::CHUNK_SIZE || ly >= crate::CHUNK_SIZE {
                return;
            }
            let wx = base_x + lx as i32;
            let wy = base_y + ly as i32;
            chunk.set(lx, ly, floor_tile(wx, wy));
        };

        for room in &rooms {
            for y in room.y..room.y + room.h {
                for x in room.x..room.x + room.w {
                    carve(x, y, &mut chunk);
                }
            }
        }

        for i in 1..rooms.len() {
            let (x1, y1) = rooms[i - 1].center();
            let (x2, y2) = rooms[i].center();
            let horizontal_first = deterministic_hash64(
                seed,
                0xAA00_11BB_22CC_33DDu64.wrapping_add(i as u64),
                chunk_x,
                chunk_y,
            ) % 2
                == 0;
            if horizontal_first {
                let (from, to) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
                for x in from..=to {
                    carve(x, y1, &mut chunk);
                }
                let (from, to) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
                for y in from..=to {
                    carve(x2, y, &mut chunk);
                }
            } else {
                let (from, to) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
                for y in from..=to {
                    carve(x1, y, &mut chunk);
                }
                let (from, to) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
                for x in from..=to {
                    carve(x, y2, &mut chunk);
                }
            }
        }

        let center = (crate::CHUNK_SIZE / 2) as usize;
        let connectors = [
            // West / East openings share keys with adjacent chunk so they match.
            (0usize, 1usize, deterministic_hash64(seed, 0x70AA_BBCC_DDEE_1001, chunk_x, chunk_y)),
            (
                crate::CHUNK_SIZE - 1,
                crate::CHUNK_SIZE - 2,
                deterministic_hash64(seed, 0x70AA_BBCC_DDEE_1001, chunk_x + 1, chunk_y),
            ),
        ];
        for (edge_x, inner_x, gate_hash) in connectors {
            if gate_hash % 100 >= 68 {
                continue;
            }
            let y = 2 + (gate_hash as usize % (crate::CHUNK_SIZE - 4));
            carve(edge_x, y, &mut chunk);
            carve(inner_x, y, &mut chunk);
            let (from, to) = if y <= center { (y, center) } else { (center, y) };
            for yy in from..=to {
                carve(inner_x, yy, &mut chunk);
            }
        }
        let vertical_connectors = [
            (0usize, 1usize, deterministic_hash64(seed, 0x7000_ABCD_EF11_2002, chunk_x, chunk_y)),
            (
                crate::CHUNK_SIZE - 1,
                crate::CHUNK_SIZE - 2,
                deterministic_hash64(seed, 0x7000_ABCD_EF11_2002, chunk_x, chunk_y + 1),
            ),
        ];
        for (edge_y, inner_y, gate_hash) in vertical_connectors {
            if gate_hash % 100 >= 68 {
                continue;
            }
            let x = 2 + (gate_hash as usize % (crate::CHUNK_SIZE - 4));
            carve(x, edge_y, &mut chunk);
            carve(x, inner_y, &mut chunk);
            let (from, to) = if x <= center { (x, center) } else { (center, x) };
            for xx in from..=to {
                carve(xx, inner_y, &mut chunk);
            }
        }

        for local_y in 0..crate::CHUNK_SIZE {
            for local_x in 0..crate::CHUNK_SIZE {
                let tile = chunk.get(local_x, local_y);
                let world_x = base_x + local_x as i32;
                let world_y = base_y + local_y as i32;
                let tile = if stairs_mode == StairsMode::Normal
                    && tile.walkable()
                    && Self::is_stairs_peak(seed, world_x, world_y)
                {
                    Tile::StairsDown
                } else {
                    tile
                };
                chunk.set(local_x, local_y, tile);
            }
        }

        chunk
    }

    fn stairs_score(seed: u64, x: i32, y: i32) -> f64 {
        deterministic_noise01(seed, NOISE_SALT_STAIRS, x, y)
    }

    fn is_stairs_peak(seed: u64, x: i32, y: i32) -> bool {
        if x * x + y * y <= STAIRS_NO_SPAWN_RADIUS2 {
            return false;
        }
        let center = Self::stairs_score(seed, x, y);
        if center < STAIRS_PEAK_THRESHOLD {
            return false;
        }
        for dy in -STAIRS_PEAK_RADIUS..=STAIRS_PEAK_RADIUS {
            for dx in -STAIRS_PEAK_RADIUS..=STAIRS_PEAK_RADIUS {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if Self::stairs_score(seed, x + dx, y + dy) >= center {
                    return false;
                }
            }
        }
        true
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

fn deterministic_noise01(seed: u64, salt: u64, x: i32, y: i32) -> f64 {
    let z = deterministic_hash64(seed, salt, x, y);
    let v = z >> 11;
    (v as f64) * (1.0 / ((1u64 << 53) as f64))
}

fn deterministic_hash64(seed: u64, salt: u64, x: i32, y: i32) -> u64 {
    // Deterministic hash(seed + coord) in [0,1).
    let ux = x as i64 as u64;
    let uy = y as i64 as u64;
    let key = seed
        .wrapping_add(salt)
        .wrapping_add(ux.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(uy.wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    let mut z = key.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    z
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
    #[serde(default)]
    player_status: StatusState,
    #[serde(default = "default_player_mp")]
    player_mp: i32,
    #[serde(default = "default_player_max_mp")]
    player_max_mp: i32,
    #[serde(default = "default_player_hunger")]
    player_hunger: i32,
    #[serde(default = "default_player_max_hunger")]
    player_max_hunger: i32,
    #[serde(default)]
    player_copper_disks: u32,
    #[serde(default, rename = "weapon_ritual_bonus", skip_serializing)]
    legacy_weapon_ritual_bonus: i32,
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
    ground_copper: Vec<GroundCopperState>,
    #[serde(default)]
    blood_stains: Vec<PosState>,
    #[serde(default)]
    torches: Vec<TorchState>,
    #[serde(default)]
    stone_tablets: Vec<StoneTabletState>,
    #[serde(default)]
    structures: Vec<StructureState>,
    #[serde(default)]
    substory_facility: Option<SubstoryFacilitySnapshotState>,
    #[serde(default)]
    substory_facility_attempted: bool,
    #[serde(default)]
    ancient_attuned_sites: Vec<PosState>,
    #[serde(default)]
    ancient_awakened_sites: Vec<PosState>,
    #[serde(default)]
    ancient_charge: u8,
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
    #[serde(default = "default_lang_code")]
    lang_code: String,
    logs: Vec<LogEntry>,
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

fn default_lang_code() -> String {
    "en".to_string()
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
    #[serde(default)]
    status: StatusState,
    #[serde(default)]
    carried_items: Vec<CarriedItem>,
    #[serde(default)]
    equipped_weapon: Option<Item>,
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
struct GroundCopperState {
    x: i32,
    y: i32,
    disks: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct TorchState {
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LogArg {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum LogEntry {
    Raw(String),
    Tr { key: String, args: Vec<LogArg> },
}

impl LogEntry {
    pub(crate) fn resolve(&self) -> String {
        match self {
            Self::Raw(text) => text.clone(),
            Self::Tr { key, args } => {
                let pairs: Vec<(String, String)> = args
                    .iter()
                    .map(|arg| (arg.name.clone(), resolve_log_arg_value(&arg.value)))
                    .collect();
                trf_map(key, &pairs)
            }
        }
    }
}

fn resolve_log_arg_value(value: &str) -> String {
    if let Some(item_key) = value.strip_prefix("\u{1f}item:") {
        if let Some(item) = Item::from_key(item_key) {
            return crate::localized_item_name(item);
        }
    }
    if let Some(creature_id) = value.strip_prefix("\u{1f}creature:") {
        return crate::localized_creature_name(creature_id);
    }
    if let Some(text_key) = value.strip_prefix("\u{1f}text:") {
        return tr(text_key).to_string();
    }
    value.to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum StructureKind {
    Altar,
    TempleCore,
    SubstoryCore,
    Terminal,
    VendingMachine,
    BoneRack,
    CablePylon,
}

impl StructureKind {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "altar" => Some(Self::Altar),
            "temple_core" => Some(Self::TempleCore),
            "substory_core" => Some(Self::SubstoryCore),
            "terminal" => Some(Self::Terminal),
            "vending_machine" => Some(Self::VendingMachine),
            "bone_rack" => Some(Self::BoneRack),
            "cable_pylon" => Some(Self::CablePylon),
            _ => None,
        }
    }

    pub(crate) fn glyph(self) -> char {
        match self {
            Self::Altar => '+',
            Self::TempleCore => '&',
            Self::SubstoryCore => '%',
            Self::Terminal => 'T',
            Self::VendingMachine => 'V',
            Self::BoneRack => 'H',
            Self::CablePylon => 'I',
        }
    }

    pub(crate) fn color(self, bright: bool) -> Color {
        let idx = match (self, bright) {
            (Self::Altar, true) => 180,
            (Self::Altar, false) => 95,
            (Self::TempleCore, true) => 223,
            (Self::TempleCore, false) => 137,
            (Self::SubstoryCore, true) => 205,
            (Self::SubstoryCore, false) => 131,
            (Self::Terminal, true) => 117,
            (Self::Terminal, false) => 67,
            (Self::VendingMachine, true) => 214,
            (Self::VendingMachine, false) => 130,
            (Self::BoneRack, true) => 250,
            (Self::BoneRack, false) => 244,
            (Self::CablePylon, true) => 221,
            (Self::CablePylon, false) => 136,
        };
        Color::Indexed(idx)
    }

    fn popup_key(self) -> &'static str {
        match self {
            Self::Altar => "structure.message.altar",
            Self::TempleCore => "structure.message.temple_core",
            Self::SubstoryCore => "structure.message.substory_core",
            Self::Terminal => "structure.message.terminal",
            Self::VendingMachine => "structure.message.vending_machine",
            Self::BoneRack => "structure.message.bone_rack",
            Self::CablePylon => "structure.message.cable_pylon",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum StoneTabletKind {
    Mercy,
    MercyLitany,
    MercyName,
    MercySumer,
    Might,
    MightWarning,
    MightName,
    MightSumer,
    Oracle,
    OracleTwins,
    OracleFifth,
    OracleLast,
    OracleSumer,
}

impl StoneTabletKind {
    fn popup_key(self) -> &'static str {
        match self {
            Self::Mercy => "tablet.message.mercy",
            Self::MercyLitany => "tablet.message.mercy_litany",
            Self::MercyName => "tablet.message.mercy_name",
            Self::MercySumer => "tablet.message.mercy_sumer",
            Self::Might => "tablet.message.might",
            Self::MightWarning => "tablet.message.might_warning",
            Self::MightName => "tablet.message.might_name",
            Self::MightSumer => "tablet.message.might_sumer",
            Self::Oracle => "tablet.message.oracle",
            Self::OracleTwins => "tablet.message.oracle_twins",
            Self::OracleFifth => "tablet.message.oracle_fifth",
            Self::OracleLast => "tablet.message.oracle_last",
            Self::OracleSumer => "tablet.message.oracle_sumer",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct StoneTabletState {
    x: i32,
    y: i32,
    kind: StoneTabletKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct StructureState {
    x: i32,
    y: i32,
    kind: StructureKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum SubstoryFacilityKind {
    PrototypeSanctum,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct SubstoryFacilitySnapshotState {
    center_x: i32,
    center_y: i32,
    kind: SubstoryFacilityKind,
    guardian_id: u64,
    cleared: bool,
}

#[derive(Clone, Copy, Debug)]
struct SubstoryFacilityState {
    center: Pos,
    kind: SubstoryFacilityKind,
    guardian_id: u64,
    cleared: bool,
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
    player_status: StatusState,
    pub(crate) player_mp: i32,
    pub(crate) player_max_mp: i32,
    pub(crate) player_hunger: i32,
    pub(crate) player_max_hunger: i32,
    pub(crate) player_copper_disks: u32,
    pub(crate) inventory: Vec<InventoryItem>,
    pub(crate) equipped_sword: Option<InventoryItem>,
    pub(crate) equipped_shield: Option<InventoryItem>,
    pub(crate) equipped_accessory: Option<InventoryItem>,
    pub(crate) enemies: Vec<Enemy>,
    pub(crate) ground_items: HashMap<(i32, i32), Item>,
    ground_copper: HashMap<(i32, i32), u32>,
    blood_stains: HashSet<(i32, i32)>,
    torches: HashSet<(i32, i32)>,
    stone_tablets: HashMap<(i32, i32), StoneTabletKind>,
    structures: HashMap<(i32, i32), StructureKind>,
    substory_facility: Option<SubstoryFacilityState>,
    substory_facility_attempted: bool,
    ancient_attuned_sites: HashSet<(i32, i32)>,
    ancient_awakened_sites: HashSet<(i32, i32)>,
    ancient_charge: u8,
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
    pub(crate) logs: Vec<LogEntry>,
    pending_dialogue: Option<String>,
    pending_popup: Option<(String, String)>,
    pending_vending: bool,
    suppress_auto_pickup_once: bool,
    invincible: bool,
    death_cause: Option<String>,
}

impl Game {
    pub(crate) fn ancient_charge(&self) -> u8 {
        self.ancient_charge
    }

    pub(crate) fn copper_weight_text(disks: u32) -> String {
        let decigrams = disks.saturating_mul(56);
        format!("{}.{:01}", decigrams / 10, decigrams % 10)
    }

    pub(crate) fn new(seed: u64) -> Self {
        let mut game = Self {
            world: World::new(seed, 1),
            player: Pos { x: 0, y: 0 },
            player_id: 0,
            facing: Facing::S,
            player_hp: creature_meta("player").hp,
            player_max_hp: creature_meta("player").hp,
            player_status: StatusState::default(),
            player_mp: PLAYER_BASE_MP,
            player_max_mp: PLAYER_BASE_MP,
            player_hunger: PLAYER_BASE_HUNGER,
            player_max_hunger: PLAYER_BASE_HUNGER,
            player_copper_disks: 0,
            inventory: Vec::new(),
            equipped_sword: None,
            equipped_shield: None,
            equipped_accessory: None,
            enemies: Vec::new(),
            ground_items: HashMap::new(),
            ground_copper: HashMap::new(),
            blood_stains: HashSet::new(),
            torches: HashSet::new(),
            stone_tablets: HashMap::new(),
            structures: HashMap::new(),
            substory_facility: None,
            substory_facility_attempted: false,
            ancient_attuned_sites: HashSet::new(),
            ancient_awakened_sites: HashSet::new(),
            ancient_charge: 0,
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
            logs: vec![LogEntry::Tr {
                key: "game.start".to_string(),
                args: Vec::new(),
            }],
            pending_dialogue: None,
            pending_popup: None,
            pending_vending: false,
            suppress_auto_pickup_once: false,
            invincible: false,
            death_cause: None,
        };
        game.player = game.find_spawn();
        game.ensure_substory_facility_generated();
        game.spawn_enemies(12);
        game.spawn_travelers(INITIAL_TRAVELER_COUNT);
        game
    }

    fn ensure_chunk_ready(&mut self, chunk_x: i32, chunk_y: i32) {
        let existed = self.world.chunks.contains_key(&(chunk_x, chunk_y));
        let base_x = chunk_x * crate::CHUNK_SIZE as i32;
        let base_y = chunk_y * crate::CHUNK_SIZE as i32;
        let _ = self.world.tile(base_x, base_y);
        if !existed {
            self.populate_items_in_chunk(chunk_x, chunk_y);
            self.populate_stone_tablets_in_chunk(chunk_x, chunk_y);
            self.populate_structures_in_chunk(chunk_x, chunk_y);
        }
    }

    pub(crate) fn tile(&mut self, x: i32, y: i32) -> Tile {
        let chunk_x = World::chunk_coord(x);
        let chunk_y = World::chunk_coord(y);
        self.ensure_chunk_ready(chunk_x, chunk_y);
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

    #[allow(dead_code)]
    pub(crate) fn floor_requires_facility_clear(&self) -> bool {
        self.world.stairs_mode == StairsMode::FacilityLocked
    }

    #[allow(dead_code)]
    pub(crate) fn floor_has_substory_facility_slot(&self) -> bool {
        self.world.special_facility_mode == SpecialFacilityMode::Substory
    }

    fn choose_substory_guardian_kind(&self) -> &'static str {
        if self.floor >= 7 {
            "cathedral_frame"
        } else if self.floor >= 4 {
            "scavenger_knight"
        } else {
            "grave_frame"
        }
    }

    fn clear_objects_in_rect(&mut self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.ground_items.remove(&(x, y));
                self.ground_copper.remove(&(x, y));
                self.torches.remove(&(x, y));
                self.stone_tablets.remove(&(x, y));
                self.structures.remove(&(x, y));
                self.blood_stains.remove(&(x, y));
            }
        }
    }

    fn can_place_substory_compound(&mut self, center_x: i32, center_y: i32) -> bool {
        for y in center_y - 4..=center_y + 4 {
            for x in center_x - 4..=center_x + 4 {
                self.ensure_chunk_ready(World::chunk_coord(x), World::chunk_coord(y));
                if x == self.player.x && y == self.player.y {
                    return false;
                }
                if self.has_enemy_at(x, y) || self.has_torch_at(x, y) {
                    return false;
                }
                let tile = self.world.tile(x, y);
                if matches!(
                    tile,
                    Tile::Abyss | Tile::DeepWater | Tile::ShallowWater | Tile::StairsDown
                ) {
                    return false;
                }
            }
        }
        true
    }

    fn spawn_substory_guardian_near(&mut self, center: Pos, creature_id: &str) -> Option<u64> {
        let candidates = [
            (0, -2),
            (2, 0),
            (0, 2),
            (-2, 0),
            (1, -2),
            (2, 1),
            (-1, 2),
            (-2, -1),
        ];
        for (dx, dy) in candidates {
            let x = center.x + dx;
            let y = center.y + dy;
            let tile = self.tile(x, y);
            if !tile.walkable()
                || tile == Tile::StairsDown
                || self.has_blocking_structure_at(x, y)
                || self.has_enemy_at(x, y)
                || (x == self.player.x && y == self.player.y)
            {
                continue;
            }
            let enemy = self.spawn_enemy_instance(x, y, creature_id);
            let id = enemy.id;
            self.enemies.push(enemy);
            return Some(id);
        }
        None
    }

    fn place_substory_compound(
        &mut self,
        center_x: i32,
        center_y: i32,
    ) -> Option<SubstoryFacilityState> {
        if !self.can_place_substory_compound(center_x, center_y) {
            return None;
        }
        self.clear_objects_in_rect(center_x - 4, center_y - 4, center_x + 4, center_y + 4);
        let entrance_side =
            deterministic_hash64(self.world.seed, 0xC4FA_7710_1119_2A55, center_x, center_y) % 4;
        for y in center_y - 4..=center_y + 4 {
            for x in center_x - 4..=center_x + 4 {
                let dx = x - center_x;
                let dy = y - center_y;
                let is_border = dx.abs() == 4 || dy.abs() == 4;
                let is_entrance = match entrance_side {
                    0 => dy == -4 && dx.abs() <= 1,
                    1 => dx == 4 && dy.abs() <= 1,
                    2 => dy == 4 && dx.abs() <= 1,
                    _ => dx == -4 && dy.abs() <= 1,
                };
                if is_border && !is_entrance {
                    self.set_tile(x, y, Tile::Wall);
                } else {
                    self.set_tile(x, y, Tile::Sand);
                }
            }
        }
        for &(px, py, kind) in &[
            (center_x - 2, center_y - 2, StructureKind::Terminal),
            (center_x + 2, center_y - 2, StructureKind::CablePylon),
            (center_x - 2, center_y + 2, StructureKind::BoneRack),
            (center_x + 2, center_y + 2, StructureKind::Terminal),
        ] {
            self.structures.insert((px, py), kind);
        }
        self.structures
            .insert((center_x, center_y), StructureKind::SubstoryCore);
        let guardian_id = self
            .spawn_substory_guardian_near(
                Pos {
                    x: center_x,
                    y: center_y,
                },
                self.choose_substory_guardian_kind(),
            )
            .unwrap_or(0);
        Some(SubstoryFacilityState {
            center: Pos {
                x: center_x,
                y: center_y,
            },
            kind: SubstoryFacilityKind::PrototypeSanctum,
            guardian_id,
            cleared: false,
        })
    }

    fn ensure_substory_facility_generated(&mut self) {
        if self.substory_facility.is_some() || self.substory_facility_attempted {
            return;
        }
        if self.world.special_facility_mode != SpecialFacilityMode::Substory
            && self.world.stairs_mode != StairsMode::FacilityLocked
        {
            self.substory_facility_attempted = true;
            return;
        }
        self.substory_facility_attempted = true;
        let radii = [18_i32, 22, 26, 30, 34, 38];
        for radius in radii {
            let salt = 0xFE11_2104_5510_9911_u64 ^ radius as u64;
            for step in 0..24_i32 {
                let roll =
                    deterministic_hash64(self.world.seed, salt, self.floor as i32, step);
                let sx = if roll & 1 == 0 { -1 } else { 1 };
                let sy = if (roll >> 1) & 1 == 0 { -1 } else { 1 };
                let jitter_x = (((roll >> 8) & 0b111) as i32) - 3;
                let jitter_y = (((roll >> 12) & 0b111) as i32) - 3;
                let x = sx * radius + jitter_x;
                let y = sy * radius + jitter_y;
                if x.abs().max(y.abs()) < 12 {
                    continue;
                }
                if let Some(state) = self.place_substory_compound(x, y) {
                    self.substory_facility = Some(state);
                    return;
                }
            }
        }
    }

    fn substory_direction_inscription(&self, from_x: i32, from_y: i32) -> Option<String> {
        let facility = self.substory_facility?;
        let dx = facility.center.x - from_x;
        let dy = facility.center.y - from_y;
        let horizontal = if dx.abs() <= 2 {
            ""
        } else if dx > 0 {
            "▶"
        } else {
            "◀"
        };
        let vertical = if dy.abs() <= 2 {
            ""
        } else if dy > 0 {
            "▼"
        } else {
            "▲"
        };
        let pattern = match (horizontal, vertical) {
            ("", "") => "◎".to_string(),
            ("", v) => format!("{v}\n│\n◎"),
            (h, "") => format!("◎─{h}"),
            (h, v) => format!("{v}\n╲\n◎─{h}"),
        };
        Some(pattern)
    }

    fn structure_popup_text(&self, x: i32, y: i32, kind: StructureKind) -> String {
        let variants: &[&str] = match kind {
            StructureKind::Altar => &[
                "structure.message.altar",
                "structure.message.altar_2",
                "structure.message.altar_3",
                "structure.message.altar_4",
            ],
            StructureKind::TempleCore => &[
                "structure.message.temple_core",
                "structure.message.temple_core_2",
                "structure.message.temple_core_3",
                "structure.message.temple_core_4",
            ],
            _ => &[kind.popup_key()],
        };
        let salt = match kind {
            StructureKind::Altar => 0x4A11_7E2D_9901_1A31,
            StructureKind::TempleCore => 0x77C4_10B2_5519_0EAF,
            StructureKind::SubstoryCore => 0xD321_A991_4410_0CE1,
            StructureKind::Terminal => 0x2C50_EE11_7741_01A7,
            StructureKind::VendingMachine => 0x6A22_6D91_17F1_0034,
            StructureKind::BoneRack => 0x34AB_1209_1D77_4C12,
            StructureKind::CablePylon => 0xA891_4EE0_7721_6301,
        };
        let idx = (deterministic_hash64(self.world.seed, salt, x, y) as usize) % variants.len();
        tr(variants[idx]).to_string()
    }

    fn complete_substory_facility(&mut self) {
        let Some(mut state) = self.substory_facility else {
            return;
        };
        if state.cleared {
            return;
        }
        state.cleared = true;
        self.substory_facility = Some(state);
        self.push_log_tr("facility.cleared");
        if self.world.stairs_mode != StairsMode::FacilityLocked {
            return;
        }
        if self.find_nearest_stairs(1024).is_some() {
            return;
        }
        if let Some(pos) = self.nearest_walkable_spot_around(state.center, 6) {
            self.set_tile(pos.x, pos.y, Tile::StairsDown);
            self.push_log_tr("facility.stairs_appeared");
        }
    }

    fn maybe_complete_substory_facility_by_guardian(&mut self, enemy_id: u64) {
        let Some(state) = self.substory_facility else {
            return;
        };
        if state.guardian_id != 0 && state.guardian_id == enemy_id {
            self.complete_substory_facility();
        }
    }

    pub(crate) fn descend_floor(&mut self) {
        self.floor = self.floor.saturating_add(1);
        let next_seed = self.next_floor_seed();
        self.world = World::new(next_seed, self.floor);
        self.ground_items.clear();
        self.ground_copper.clear();
        self.blood_stains.clear();
        self.torches.clear();
        self.stone_tablets.clear();
        self.structures.clear();
        self.substory_facility = None;
        self.substory_facility_attempted = false;
        self.enemies.clear();
        self.attack_effects.clear();
        self.visual_effects.clear();
        self.pending_enemy_hits.clear();
        self.harvest_state = None;
        self.player_mp = self.player_max_mp;
        self.player = self.find_spawn();
        self.ensure_substory_facility_generated();
        self.spawn_enemies(12);
        self.spawn_travelers(INITIAL_TRAVELER_COUNT);
        self.push_log_trf(
            "game.descended_floor",
            &[("floor", self.floor.to_string())],
        );
    }

    fn is_item_peak(&self, x: i32, y: i32) -> bool {
        if x * x + y * y <= ITEM_NO_SPAWN_RADIUS2 {
            return false;
        }
        let center = deterministic_noise01(self.world.seed, NOISE_SALT_ITEMS_PEAK, x, y);
        if center < ITEM_PEAK_THRESHOLD {
            return false;
        }
        for dy in -ITEM_PEAK_RADIUS..=ITEM_PEAK_RADIUS {
            for dx in -ITEM_PEAK_RADIUS..=ITEM_PEAK_RADIUS {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let v =
                    deterministic_noise01(self.world.seed, NOISE_SALT_ITEMS_PEAK, x + dx, y + dy);
                if v >= center {
                    return false;
                }
            }
        }
        true
    }

    fn choose_weighted_item_from_pool_deterministic(
        &self,
        x: i32,
        y: i32,
        pool: &[(Item, u32)],
    ) -> Option<Item> {
        if pool.is_empty() {
            return None;
        }
        let total_weight: u32 = pool.iter().map(|(_, w)| *w).sum();
        if total_weight == 0 {
            return None;
        }
        let noise = deterministic_noise01(self.world.seed, NOISE_SALT_ITEMS_PICK, x, y);
        let mut r = (noise * total_weight as f64).floor() as u32;
        if r >= total_weight {
            r = total_weight.saturating_sub(1);
        }
        for (item, w) in pool {
            if r < *w {
                return Some(*item);
            }
            r -= *w;
        }
        pool.last().map(|(item, _)| *item)
    }

    fn populate_items_in_chunk(&mut self, chunk_x: i32, chunk_y: i32) {
        let configured_drop_pool = crate::world_cfg::drop_pool_for_floor(self.floor);
        let base_x = chunk_x * crate::CHUNK_SIZE as i32;
        let base_y = chunk_y * crate::CHUNK_SIZE as i32;
        for local_y in 0..crate::CHUNK_SIZE {
            for local_x in 0..crate::CHUNK_SIZE {
                let x = base_x + local_x as i32;
                let y = base_y + local_y as i32;
                if self.item_at(x, y).is_some() {
                    continue;
                }
                let tile = self.world.tile(x, y);
                if !tile.walkable() || tile == Tile::StairsDown {
                    continue;
                }
                if x == self.player.x && y == self.player.y {
                    continue;
                }
                if !self.is_item_peak(x, y) {
                    continue;
                }
                let chosen = if configured_drop_pool.is_empty() {
                    let biome = self.world.biome_id_at(x, y);
                    self.choose_weighted_item_from_pool_deterministic(x, y, biome_item_pool(biome))
                } else {
                    self.choose_weighted_item_from_pool_deterministic(x, y, &configured_drop_pool)
                };
                if let Some(item) = chosen {
                    self.ground_items.insert((x, y), item);
                }
            }
        }
    }

    fn choose_tablet_kind_for_chunk(&self, chunk_x: i32, chunk_y: i32) -> StoneTabletKind {
        let roll = deterministic_hash64(self.world.seed, NOISE_SALT_TABLET_PLACE, chunk_x, chunk_y);
        let bucket = (roll % 13) as usize;
        if self.floor >= 5 {
            const DEEP: [StoneTabletKind; 13] = [
                StoneTabletKind::Oracle,
                StoneTabletKind::OracleTwins,
                StoneTabletKind::OracleFifth,
                StoneTabletKind::OracleLast,
                StoneTabletKind::OracleSumer,
                StoneTabletKind::Might,
                StoneTabletKind::MightWarning,
                StoneTabletKind::MightSumer,
                StoneTabletKind::Mercy,
                StoneTabletKind::MercyLitany,
                StoneTabletKind::MercySumer,
                StoneTabletKind::MightName,
                StoneTabletKind::MercyName,
            ];
            DEEP[bucket]
        } else if self.floor >= 3 {
            const MID: [StoneTabletKind; 13] = [
                StoneTabletKind::Mercy,
                StoneTabletKind::MercyLitany,
                StoneTabletKind::MercySumer,
                StoneTabletKind::Might,
                StoneTabletKind::MightWarning,
                StoneTabletKind::MightSumer,
                StoneTabletKind::Oracle,
                StoneTabletKind::OracleTwins,
                StoneTabletKind::OracleFifth,
                StoneTabletKind::OracleSumer,
                StoneTabletKind::MercyName,
                StoneTabletKind::MightName,
                StoneTabletKind::OracleLast,
            ];
            MID[bucket]
        } else {
            const SHALLOW: [StoneTabletKind; 13] = [
                StoneTabletKind::Mercy,
                StoneTabletKind::Mercy,
                StoneTabletKind::MercyLitany,
                StoneTabletKind::MercySumer,
                StoneTabletKind::MercyName,
                StoneTabletKind::Might,
                StoneTabletKind::Might,
                StoneTabletKind::MightWarning,
                StoneTabletKind::MightSumer,
                StoneTabletKind::MightName,
                StoneTabletKind::Oracle,
                StoneTabletKind::OracleTwins,
                StoneTabletKind::OracleSumer,
            ];
            SHALLOW[bucket]
        }
    }

    fn populate_stone_tablets_in_chunk(&mut self, chunk_x: i32, chunk_y: i32) {
        let spawn_roll =
            deterministic_noise01(self.world.seed, NOISE_SALT_TABLET_PLACE, chunk_x, chunk_y);
        if spawn_roll < 0.84 {
            return;
        }

        let base_x = chunk_x * crate::CHUNK_SIZE as i32;
        let base_y = chunk_y * crate::CHUNK_SIZE as i32;
        let mut best: Option<((i32, i32), f64)> = None;
        for local_y in 0..crate::CHUNK_SIZE {
            for local_x in 0..crate::CHUNK_SIZE {
                let x = base_x + local_x as i32;
                let y = base_y + local_y as i32;
                if self.item_at(x, y).is_some() || self.stone_tablets.contains_key(&(x, y)) {
                    continue;
                }
                let tile = self.world.tile(x, y);
                if !tile.walkable() || tile == Tile::StairsDown {
                    continue;
                }
                if x == self.player.x && y == self.player.y {
                    continue;
                }
                let score = deterministic_noise01(
                    self.world.seed,
                    NOISE_SALT_TABLET_PLACE ^ 0x55AA_9C3D_2E10_7711,
                    x,
                    y,
                );
                if best.as_ref().is_none_or(|(_, cur)| score > *cur) {
                    best = Some(((x, y), score));
                }
            }
        }
        if let Some(((x, y), _)) = best {
            self.stone_tablets
                .insert((x, y), self.choose_tablet_kind_for_chunk(chunk_x, chunk_y));
        }
    }

    fn choose_weighted_structure_from_pool_deterministic(
        &self,
        x: i32,
        y: i32,
        pool: &[(String, u32)],
    ) -> Option<StructureKind> {
        if pool.is_empty() {
            return None;
        }
        let total_weight: u32 = pool.iter().map(|(_, w)| *w).sum();
        if total_weight == 0 {
            return None;
        }
        let noise = deterministic_noise01(self.world.seed, NOISE_SALT_STRUCTURE_PLACE, x, y);
        let mut r = (noise * total_weight as f64).floor() as u32;
        if r >= total_weight {
            r = total_weight.saturating_sub(1);
        }
        for (id, w) in pool {
            if r < *w {
                return StructureKind::from_key(id);
            }
            r -= *w;
        }
        pool.last()
            .and_then(|(id, _)| StructureKind::from_key(id.as_str()))
    }

    fn can_place_altar_compound(&mut self, chunk_x: i32, chunk_y: i32, center_x: i32, center_y: i32) -> bool {
        for y in center_y - 2..=center_y + 2 {
            for x in center_x - 2..=center_x + 2 {
                if World::chunk_coord(x) != chunk_x || World::chunk_coord(y) != chunk_y {
                    return false;
                }
                if (x == self.player.x && y == self.player.y)
                    || self.has_enemy_at(x, y)
                    || self.item_at(x, y).is_some()
                    || self.has_torch_at(x, y)
                    || self.stone_tablets.contains_key(&(x, y))
                    || self.structures.contains_key(&(x, y))
                {
                    return false;
                }
                let tile = self.world.tile(x, y);
                if matches!(tile, Tile::Abyss | Tile::DeepWater | Tile::ShallowWater | Tile::StairsDown) {
                    return false;
                }
            }
        }
        true
    }

    fn place_altar_compound(&mut self, chunk_x: i32, chunk_y: i32, center_x: i32, center_y: i32) -> bool {
        if !self.can_place_altar_compound(chunk_x, chunk_y, center_x, center_y) {
            return false;
        }
        let entrance_side =
            deterministic_hash64(self.world.seed, 0x91AF_2201_7711_55CC, center_x, center_y) % 4;
        for y in center_y - 2..=center_y + 2 {
            for x in center_x - 2..=center_x + 2 {
                let dx = x - center_x;
                let dy = y - center_y;
                let is_border = dx.abs() == 2 || dy.abs() == 2;
                let is_entrance = match entrance_side {
                    0 => dx == 0 && dy == -2,
                    1 => dx == 2 && dy == 0,
                    2 => dx == 0 && dy == 2,
                    _ => dx == -2 && dy == 0,
                };
                if is_border && !is_entrance {
                    self.set_tile(x, y, Tile::Wall);
                } else {
                    self.set_tile(x, y, Tile::Sand);
                }
            }
        }
        self.structures.insert((center_x, center_y), StructureKind::Altar);
        true
    }

    fn can_place_temple_compound(
        &mut self,
        chunk_x: i32,
        chunk_y: i32,
        center_x: i32,
        center_y: i32,
    ) -> bool {
        for y in center_y - 3..=center_y + 3 {
            for x in center_x - 3..=center_x + 3 {
                if World::chunk_coord(x) != chunk_x || World::chunk_coord(y) != chunk_y {
                    return false;
                }
                if (x == self.player.x && y == self.player.y)
                    || self.has_enemy_at(x, y)
                    || self.item_at(x, y).is_some()
                    || self.has_torch_at(x, y)
                    || self.stone_tablets.contains_key(&(x, y))
                    || self.structures.contains_key(&(x, y))
                {
                    return false;
                }
                let tile = self.world.tile(x, y);
                if matches!(tile, Tile::Abyss | Tile::DeepWater | Tile::ShallowWater | Tile::StairsDown) {
                    return false;
                }
            }
        }
        true
    }

    fn place_temple_compound(
        &mut self,
        chunk_x: i32,
        chunk_y: i32,
        center_x: i32,
        center_y: i32,
    ) -> bool {
        if !self.can_place_temple_compound(chunk_x, chunk_y, center_x, center_y) {
            return false;
        }
        let entrance_side =
            deterministic_hash64(self.world.seed, 0xD1AF_2201_7711_55CC, center_x, center_y) % 4;
        for y in center_y - 3..=center_y + 3 {
            for x in center_x - 3..=center_x + 3 {
                let dx = x - center_x;
                let dy = y - center_y;
                let is_border = dx.abs() == 3 || dy.abs() == 3;
                let is_entrance = match entrance_side {
                    0 => dy == -3 && dx.abs() <= 1,
                    1 => dx == 3 && dy.abs() <= 1,
                    2 => dy == 3 && dx.abs() <= 1,
                    _ => dx == -3 && dy.abs() <= 1,
                };
                if is_border && !is_entrance {
                    self.set_tile(x, y, Tile::Wall);
                } else {
                    self.set_tile(x, y, Tile::Sand);
                }
            }
        }
        for &(px, py) in &[
            (center_x - 1, center_y - 1),
            (center_x + 1, center_y - 1),
            (center_x - 1, center_y + 1),
            (center_x + 1, center_y + 1),
        ] {
            self.set_tile(px, py, Tile::Wall);
        }
        self.structures
            .insert((center_x, center_y), StructureKind::TempleCore);
        true
    }

    fn populate_structures_in_chunk(&mut self, chunk_x: i32, chunk_y: i32) {
        let configured_pool = crate::world_cfg::structure_pool_for_floor(self.floor);
        if configured_pool.is_empty() {
            return;
        }
        let threshold = match self.floor {
            1..=2 => 0.95,
            3..=4 => 0.92,
            5..=6 => 0.90,
            7..=8 => 0.87,
            9..=10 => 0.84,
            11..=20 => 0.78,
            _ => 0.68,
        };
        let spawn_roll = deterministic_noise01(
            self.world.seed,
            NOISE_SALT_STRUCTURE_PLACE ^ 0xA55A_4D20_3311_8877,
            chunk_x,
            chunk_y,
        );
        if spawn_roll < threshold {
            return;
        }

        let base_x = chunk_x * crate::CHUNK_SIZE as i32;
        let base_y = chunk_y * crate::CHUNK_SIZE as i32;
        let mut candidates: Vec<((i32, i32), f64)> = Vec::new();
        for local_y in 0..crate::CHUNK_SIZE {
            for local_x in 0..crate::CHUNK_SIZE {
                let x = base_x + local_x as i32;
                let y = base_y + local_y as i32;
                if self.item_at(x, y).is_some()
                    || self.stone_tablets.contains_key(&(x, y))
                    || self.structures.contains_key(&(x, y))
                {
                    continue;
                }
                let tile = self.world.tile(x, y);
                if !tile.walkable() || tile == Tile::StairsDown {
                    continue;
                }
                if x == self.player.x && y == self.player.y {
                    continue;
                }
                let score = deterministic_noise01(
                    self.world.seed,
                    NOISE_SALT_STRUCTURE_PLACE ^ 0x19C4_DD77_9812_4001,
                    x,
                    y,
                );
                candidates.push(((x, y), score));
            }
        }
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
        for ((x, y), _) in candidates {
            let Some(kind) =
                self.choose_weighted_structure_from_pool_deterministic(x, y, &configured_pool)
            else {
                continue;
            };
            match kind {
                StructureKind::Altar => {
                    if self.place_altar_compound(chunk_x, chunk_y, x, y) {
                        return;
                    }
                }
                StructureKind::TempleCore => {
                    if self.place_temple_compound(chunk_x, chunk_y, x, y) {
                        return;
                    }
                }
                _ => {
                    self.structures.insert((x, y), kind);
                    return;
                }
            }
        }
    }

    fn alloc_entity_id(&mut self) -> u64 {
        let id = self.next_entity_id.max(1);
        self.next_entity_id = id.saturating_add(1);
        id
    }

    fn find_spawn(&mut self) -> Pos {
        if self.tile(0, 0).walkable()
            && self.tile(0, 0) != Tile::StairsDown
            && !self.has_blocking_structure_at(0, 0)
        {
            return Pos { x: 0, y: 0 };
        }

        for radius in 1..=128_i32 {
            for y in -radius..=radius {
                for x in -radius..=radius {
                    let tile = self.tile(x, y);
                    if tile.walkable() && tile != Tile::StairsDown && !self.has_blocking_structure_at(x, y) {
                        return Pos { x, y };
                    }
                }
            }
        }

        Pos { x: 0, y: 0 }
    }

    fn try_move(&mut self, dx: i32, dy: i32) -> MoveResult {
        let suppress_pickup = self.suppress_auto_pickup_once;
        self.suppress_auto_pickup_once = false;
        let old_facing = self.facing;
        if let Some(facing) = Facing::from_delta(dx, dy) {
            self.facing = facing;
        }
        let nx = self.player.x + dx;
        let ny = self.player.y + dy;
        if let Some(i) = self
            .enemies
            .iter()
            .position(|e| e.pos.x == nx && e.pos.y == ny)
        {
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
            self.push_log_tr("game.enemy_blocks");
            return MoveResult::Blocked;
        }
        if self.tile(nx, ny).walkable() && !self.has_blocking_structure_at(nx, ny) {
            self.player = Pos { x: nx, y: ny };
            self.stat_steps = self.stat_steps.saturating_add(1);
            if !suppress_pickup {
                self.pick_up_item_at_player();
            }
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
            Action::Wait => {}
        }
        if consume_turn {
            if !keep_harvest_chain {
                self.harvest_state = None;
            }
            self.consume_turn();
        }
    }

    pub(crate) fn suppress_auto_pickup_once(&mut self) {
        self.suppress_auto_pickup_once = true;
    }

    pub(crate) fn push_log<S: Into<String>>(&mut self, msg: S) {
        self.logs.push(LogEntry::Raw(msg.into()));
        if self.logs.len() > 300 {
            self.logs.drain(0..100);
        }
    }

    pub(crate) fn push_log_tr(&mut self, key: &str) {
        self.logs.push(LogEntry::Tr {
            key: key.to_string(),
            args: Vec::new(),
        });
        if self.logs.len() > 300 {
            self.logs.drain(0..100);
        }
    }

    pub(crate) fn push_log_trf(&mut self, key: &str, args: &[(&str, String)]) {
        self.logs.push(LogEntry::Tr {
            key: key.to_string(),
            args: args
                .iter()
                .map(|(name, value)| LogArg {
                    name: (*name).to_string(),
                    value: value.clone(),
                })
                .collect(),
        });
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

    fn queue_popup<T: Into<String>, S: Into<String>>(&mut self, title: T, text: S) {
        self.pending_popup = Some((title.into(), text.into()));
    }

    pub(crate) fn take_pending_popup(&mut self) -> Option<(String, String)> {
        self.pending_popup.take()
    }

    fn queue_vending(&mut self) {
        self.pending_vending = true;
    }

    pub(crate) fn take_pending_vending(&mut self) -> bool {
        let out = self.pending_vending;
        self.pending_vending = false;
        out
    }

    pub(crate) fn set_invincible(&mut self, enabled: bool) {
        self.invincible = enabled;
    }

    pub(crate) fn invincible(&self) -> bool {
        self.invincible
    }

    pub(crate) fn death_cause_text(&self) -> String {
        self.death_cause
            .clone()
            .unwrap_or_else(|| tr("death.cause.unknown").to_string())
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
                let enemy_agi = self.enemy_agility(i);
                let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
                if self.roll_percent(evade_chance_percent(self.player_agility(), enemy_agi)) {
                    self.push_log_trf(
                        "game.enemy_evaded",
                        &[("enemy", crate::log_arg_creature_ref(&self.enemies[i].creature_id))],
                    );
                    return false;
                }
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
                if self.enemies[i].hp <= 0 {
                    self.stat_enemies_defeated = self.stat_enemies_defeated.saturating_add(1);
                    let dead = self.enemies.remove(i);
                    self.maybe_complete_substory_facility_by_guardian(dead.id);
                    self.blood_stains.insert((dead.pos.x, dead.pos.y));
                    self.push_death_cry(&dead.creature_id);
                    self.push_log_trf(
                        "game.you_defeated",
                        &[("enemy", crate::log_arg_creature_ref(&dead.creature_id))],
                    );
                    let cdef = creature_meta(&dead.creature_id);
                    let gained = (cdef.hp.max(1) + cdef.attack + cdef.defense * 2).max(1) as u32;
                    self.gain_exp(gained);
                    if Self::is_traveler_id(&dead.creature_id) {
                        self.maybe_drop_traveler_bread(dead.pos);
                    } else {
                        self.maybe_drop_enemy_carried_item(&dead);
                    }
                } else {
                    self.push_log_trf(
                        "game.you_hit_enemy",
                        &[
                            ("enemy", crate::log_arg_creature_ref(&self.enemies[i].creature_id)),
                            ("damage", damage.to_string()),
                        ],
                    );
                }
                false
            }
            None => {
                if let Some(kind) = self.stone_tablet_at(tx, ty) {
                    self.harvest_state = None;
                    let text = self
                        .substory_direction_inscription(tx, ty)
                        .unwrap_or_else(|| tr(kind.popup_key()).to_string());
                    let _ = self.absorb_faith_trace(tx, ty);
                    self.queue_popup(tr("object.stone_tablet").to_string(), text);
                    return true;
                }
                if let Some(kind) = self.structure_at(tx, ty) {
                    self.harvest_state = None;
                    if kind == StructureKind::VendingMachine {
                        self.queue_vending();
                        return true;
                    }
                    let title_key = match kind {
                        StructureKind::Altar => "object.altar",
                        StructureKind::TempleCore => "object.temple_core",
                        StructureKind::SubstoryCore => "object.substory_core",
                        StructureKind::Terminal => "object.terminal",
                        StructureKind::VendingMachine => "object.vending_machine",
                        StructureKind::BoneRack => "object.bone_rack",
                        StructureKind::CablePylon => "object.cable_pylon",
                    };
                    let text = self.structure_popup_text(tx, ty, kind);
                    let _ = self.absorb_faith_trace(tx, ty);
                    if let Some(answer) = self.try_awaken_ancient_site(tx, ty, kind) {
                        self.push_log(answer);
                    }
                    self.queue_popup(tr(title_key).to_string(), text);
                    return true;
                }
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
                            self.push_log_trf(
                                "game.broke_to_item",
                                &[
                                    ("target", label.to_string()),
                                    ("item", crate::log_arg_item_ref(Item::Torch)),
                                ],
                            );
                        } else {
                            self.push_log_trf("game.broke", &[("target", label.to_string())]);
                        }
                    } else {
                        self.harvest_state = Some(HarvestState {
                            target: (tx, ty),
                            hits,
                        });
                        self.push_log_trf(
                            "game.damaged",
                            &[
                                ("target", label.to_string()),
                                ("hits", hits.to_string()),
                                ("max", durability.to_string()),
                            ],
                        );
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
                                self.push_log_trf(
                                    "game.broke_to_item",
                                    &[
                                        ("target", label.to_string()),
                                        ("item", crate::log_arg_item_ref(item)),
                                    ],
                                );
                            } else {
                                self.push_log_trf("game.broke", &[("target", label.to_string())]);
                            }
                        } else {
                            self.push_log_trf("game.broke", &[("target", label.to_string())]);
                        }
                    } else {
                        self.harvest_state = Some(HarvestState {
                            target: (tx, ty),
                            hits,
                        });
                        self.push_log_trf(
                            "game.damaged",
                            &[
                                ("target", label.to_string()),
                                ("hits", hits.to_string()),
                                ("max", durability.to_string()),
                            ],
                        );
                    }
                    true
                } else {
                    self.push_log_tr("game.no_target");
                    false
                }
            }
        }
    }

    pub(crate) fn has_enemy_at(&self, x: i32, y: i32) -> bool {
        self.enemies.iter().any(|e| e.pos.x == x && e.pos.y == y)
    }

    pub(crate) fn teleport_player(&mut self, x: i32, y: i32) -> Result<(), String> {
        if !self.tile(x, y).walkable() || self.has_blocking_structure_at(x, y) {
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

    fn nearest_walkable_spot_around(&mut self, center: Pos, radius: i32) -> Option<Pos> {
        let mut best: Option<(Pos, i32)> = None;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let x = center.x + dx;
                let y = center.y + dy;
                let dist = dx * dx + dy * dy;
                if dist == 0 {
                    continue;
                }
                let tile = self.tile(x, y);
                if !tile.walkable()
                    || tile == Tile::StairsDown
                    || self.has_blocking_structure_at(x, y)
                    || self.has_enemy_at(x, y)
                {
                    continue;
                }
                if best.as_ref().is_none_or(|(_, cur)| dist < *cur) {
                    best = Some((Pos { x, y }, dist));
                }
            }
        }
        best.map(|(pos, _)| pos)
    }

    pub(crate) fn find_nearest_structure_approach(
        &mut self,
        kinds: &[StructureKind],
        max_radius: i32,
    ) -> Option<Pos> {
        for r in 0..=max_radius.max(0) {
            let mut best: Option<(Pos, i32)> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let x = self.player.x + dx;
                    let y = self.player.y + dy;
                    let Some(kind) = self.structure_at(x, y) else {
                        continue;
                    };
                    if !kinds.contains(&kind) {
                        continue;
                    }
                    let Some(approach) = self.nearest_walkable_spot_around(Pos { x, y }, 4) else {
                        continue;
                    };
                    let dist = dx * dx + dy * dy;
                    if best.as_ref().is_none_or(|(_, cur)| dist < *cur) {
                        best = Some((approach, dist));
                    }
                }
            }
            if let Some((pos, _)) = best {
                return Some(pos);
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

    pub(crate) fn copper_at(&self, x: i32, y: i32) -> Option<u32> {
        self.ground_copper.get(&(x, y)).copied()
    }

    pub(crate) fn ground_item_at_player(&self) -> Option<Item> {
        self.item_at(self.player.x, self.player.y)
    }

    pub(crate) fn has_blood_stain(&self, x: i32, y: i32) -> bool {
        self.blood_stains.contains(&(x, y))
    }

    pub(crate) fn has_torch_at(&self, x: i32, y: i32) -> bool {
        self.torches.contains(&(x, y))
    }

    fn status_list(status: StatusState) -> Vec<String> {
        let mut out = Vec::new();
        if status.burning_turns > 0 {
            out.push(tr("effect.burning").to_string());
        }
        if status.slowed_turns > 0 {
            out.push(tr("effect.slowed").to_string());
        }
        out
    }

    pub(crate) fn player_status_summary(&self) -> String {
        let list = Self::status_list(self.player_status);
        if list.is_empty() {
            tr("status.none").to_string()
        } else {
            list.join(", ")
        }
    }

    fn absorb_faith_trace(&mut self, x: i32, y: i32) -> bool {
        if self.ancient_attuned_sites.insert((x, y)) {
            self.ancient_charge = self.ancient_charge.saturating_add(1).min(9);
            self.push_log_tr("ancient.trace_gain");
            true
        } else {
            false
        }
    }

    fn ancient_activation_cost(kind: StructureKind) -> u8 {
        match kind {
            StructureKind::Altar => 2,
            StructureKind::TempleCore => 4,
            _ => 0,
        }
    }

    fn choose_ancient_reward(&self, kind: StructureKind, x: i32, y: i32) -> Item {
        let roll = deterministic_hash64(self.world.seed, 0xAE11_CE77_4402_1001, x, y) % 100;
        match kind {
            StructureKind::Altar => {
                if self.floor <= 4 {
                    Item::VirgaOriens
                } else if self.floor <= 6 {
                    if roll < 65 { Item::VirgaOriens } else { Item::FerrumOccasus }
                } else if roll < 55 {
                    Item::FerrumOccasus
                } else {
                    Item::VirgaMeridies
                }
            }
            StructureKind::TempleCore => {
                if self.floor <= 8 {
                    if roll < 55 { Item::VirgaMeridies } else { Item::FerrumOccasus }
                } else if roll < 40 {
                    Item::GladiusNadir
                } else if roll < 75 {
                    Item::VirgaZenith
                } else {
                    Item::VirgaMeridies
                }
            }
            _ => Item::RepulseScroll,
        }
    }

    fn try_awaken_ancient_site(&mut self, x: i32, y: i32, kind: StructureKind) -> Option<String> {
        let cost = Self::ancient_activation_cost(kind);
        if cost == 0 {
            return None;
        }
        if self.ancient_awakened_sites.contains(&(x, y)) {
            return Some(tr("ancient.spent").to_string());
        }
        if self.ancient_charge < cost {
            return Some(tr("ancient.insufficient").to_string());
        }
        self.ancient_charge = self.ancient_charge.saturating_sub(cost);
        self.ancient_awakened_sites.insert((x, y));
        let reward = self.choose_ancient_reward(kind, x, y);
        let _ = self.place_ground_item_near(x, y, reward);
        self.push_log_trf("ancient.unseal", &[("item", crate::log_arg_item_ref(reward))]);
        Some(trf(
            "ancient.answer",
            &[("item", crate::localized_item_name(reward))],
        ))
    }

    fn has_blocking_structure_at(&self, x: i32, y: i32) -> bool {
        self.stone_tablets.contains_key(&(x, y)) || self.structures.contains_key(&(x, y))
    }

    pub(crate) fn stone_tablet_at(&self, x: i32, y: i32) -> Option<StoneTabletKind> {
        self.stone_tablets.get(&(x, y)).copied()
    }

    pub(crate) fn structure_at(&self, x: i32, y: i32) -> Option<StructureKind> {
        self.structures.get(&(x, y)).copied()
    }

    pub(crate) fn debug_place_tile_ahead(&mut self, tile: Tile) -> Result<(i32, i32), String> {
        let (dx, dy) = self.facing.delta();
        let x = self.player.x + dx;
        let y = self.player.y + dy;
        if self.has_enemy_at(x, y) || (self.player.x == x && self.player.y == y) {
            return Err(tr("debug.place_blocked").to_string());
        }
        self.ground_items.remove(&(x, y));
        self.ground_copper.remove(&(x, y));
        self.torches.remove(&(x, y));
        self.stone_tablets.remove(&(x, y));
        self.structures.remove(&(x, y));
        self.blood_stains.remove(&(x, y));
        self.set_tile(x, y, tile);
        Ok((x, y))
    }

    pub(crate) fn debug_place_tablet_ahead(
        &mut self,
        kind: StoneTabletKind,
    ) -> Result<(i32, i32), String> {
        let (dx, dy) = self.facing.delta();
        let x = self.player.x + dx;
        let y = self.player.y + dy;
        let tile = self.tile(x, y);
        if self.has_enemy_at(x, y)
            || tile == Tile::StairsDown
            || !tile.walkable()
            || (self.player.x == x && self.player.y == y)
        {
            return Err(tr("debug.place_blocked").to_string());
        }
        self.ground_items.remove(&(x, y));
        self.ground_copper.remove(&(x, y));
        self.torches.remove(&(x, y));
        self.stone_tablets.insert((x, y), kind);
        Ok((x, y))
    }

    pub(crate) fn debug_place_structure_ahead(
        &mut self,
        kind: StructureKind,
    ) -> Result<(i32, i32), String> {
        let (dx, dy) = self.facing.delta();
        let x = self.player.x + dx;
        let y = self.player.y + dy;
        let tile = self.tile(x, y);
        if self.has_enemy_at(x, y)
            || tile == Tile::StairsDown
            || !tile.walkable()
            || (self.player.x == x && self.player.y == y)
        {
            return Err(tr("debug.place_blocked").to_string());
        }
        self.ground_items.remove(&(x, y));
        self.ground_copper.remove(&(x, y));
        self.torches.remove(&(x, y));
        self.stone_tablets.remove(&(x, y));
        self.structures.insert((x, y), kind);
        Ok((x, y))
    }

    pub(crate) fn debug_place_temple_ahead(&mut self) -> Result<(i32, i32), String> {
        let (dx, dy) = self.facing.delta();
        let x = self.player.x + dx;
        let y = self.player.y + dy;
        let chunk_x = World::chunk_coord(x);
        let chunk_y = World::chunk_coord(y);
        if self.place_temple_compound(chunk_x, chunk_y, x, y) {
            Ok((x, y))
        } else {
            Err(tr("debug.place_blocked").to_string())
        }
    }

    pub(crate) fn is_lit_by_torch(&mut self, x: i32, y: i32) -> bool {
        let r2 = TORCH_LIGHT_RADIUS * TORCH_LIGHT_RADIUS;
        let torches: Vec<(i32, i32)> = self.torches.iter().copied().collect();
        torches.into_iter().any(|(tx, ty)| {
            let dx = tx - x;
            let dy = ty - y;
            dx * dx + dy * dy <= r2 && self.has_line_of_sight(Pos { x: tx, y: ty }, Pos { x, y })
        })
    }

    pub(crate) fn inventory_len(&self) -> usize {
        self.inventory.len()
    }

    pub(crate) fn inventory_item_name(&self, idx: usize) -> Option<String> {
        self.inventory.get(idx).map(InventoryItem::display_name)
    }

    pub(crate) fn move_inventory_item(&mut self, from: usize, to: usize) -> bool {
        if from >= self.inventory.len() || to >= self.inventory.len() || from == to {
            return false;
        }
        let item = self.inventory.remove(from);
        self.inventory.insert(to, item);
        true
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
        let taken = if self.inventory[idx].qty > 1 {
            self.inventory[idx].qty = self.inventory[idx].qty.saturating_sub(1);
            let mut one = self.inventory[idx].clone();
            one.qty = 1;
            one
        } else {
            self.inventory.remove(idx)
        };
        self.sync_equipped_with_inventory();
        Some(taken)
    }

    fn sync_equipped_with_inventory(&mut self) {
        let has_item = |inv: &Vec<InventoryItem>, equipped: &InventoryItem| {
            inv.iter().any(|it| it.same_identity(equipped))
        };
        if self
            .equipped_sword
            .as_ref()
            .is_some_and(|eq| !has_item(&self.inventory, eq))
        {
            self.equipped_sword = None;
        }
        if self
            .equipped_shield
            .as_ref()
            .is_some_and(|eq| !has_item(&self.inventory, eq))
        {
            self.equipped_shield = None;
        }
        if self
            .equipped_accessory
            .as_ref()
            .is_some_and(|eq| !has_item(&self.inventory, eq))
        {
            self.equipped_accessory = None;
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
                let uid = self.alloc_entity_id();
                self.inventory.push(InventoryItem {
                    uid,
                    kind: item.kind,
                    custom_name: None,
                    weapon_bonus: 0,
                    qty: add,
                });
                remaining = remaining.saturating_sub(add);
            }
            return true;
        }
        item.qty = 1;
        if item.uid == 0 {
            item.uid = self.alloc_entity_id();
        }
        if self.inventory_full() {
            return false;
        }
        self.inventory.push(item);
        true
    }

    pub(crate) fn add_item_kind_to_inventory(&mut self, kind: Item) -> bool {
        self.add_item_to_inventory(InventoryItem {
            uid: 0,
            kind,
            custom_name: None,
            weapon_bonus: 0,
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
            if self.item_at(x, y).is_none() && self.copper_at(x, y).is_none() {
                self.ground_items.insert((x, y), kind);
                return true;
            }
        }
        false
    }

    fn place_ground_copper_near(&mut self, origin_x: i32, origin_y: i32, disks: u32) -> bool {
        if disks == 0 {
            return false;
        }
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
            if self.item_at(x, y).is_none() && self.copper_at(x, y).is_none() {
                self.ground_copper.insert((x, y), disks);
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
            self.push_log_trf("game.drop_full", &[("item", crate::log_arg_inventory_item_ref(&item))]);
        } else {
            self.push_log_trf("game.lost_item", &[("item", crate::log_arg_inventory_item_ref(&item))]);
        }
    }

    pub(crate) fn pick_up_item_at_player(&mut self) -> bool {
        let key = (self.player.x, self.player.y);
        let picked = self.ground_items.get(&key).copied();
        if let Some(item) = picked {
            if self.add_item_kind_to_inventory(item) {
                self.ground_items.remove(&key);
            } else {
                self.push_log_tr("game.inv_full");
                return false;
            }
            self.stat_items_picked = self.stat_items_picked.saturating_add(1);
            self.push_log_trf(
                "game.picked",
                &[
                    ("item", crate::log_arg_item_ref(item)),
                    ("count", self.inventory.len().to_string()),
                    ("max", crate::MAX_INVENTORY.to_string()),
                ],
            );
            return true;
        }
        if let Some(disks) = self.ground_copper.remove(&key) {
            self.player_copper_disks = self.player_copper_disks.saturating_add(disks);
            self.push_log_trf(
                "game.picked_copper",
                &[
                    ("grams", Self::copper_weight_text(disks)),
                    ("total", Self::copper_weight_text(self.player_copper_disks)),
                ],
            );
            return true;
        }
        false
    }

    pub(crate) fn pick_up_item_at_player_kind(&mut self) -> Option<Item> {
        let kind = self.ground_item_at_player()?;
        if self.pick_up_item_at_player() {
            Some(kind)
        } else {
            None
        }
    }

    pub(crate) fn first_inventory_index_of_kind(&self, kind: Item) -> Option<usize> {
        self.inventory.iter().position(|it| it.kind == kind)
    }

    pub(crate) fn swap_ground_item_with_inventory(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            return false;
        }
        let key = (self.player.x, self.player.y);
        let Some(ground_item) = self.ground_items.get(&key).copied() else {
            return false;
        };
        let Some(inv_item) = self.take_inventory_one(idx) else {
            return false;
        };
        if !self.add_item_kind_to_inventory(ground_item) {
            let _ = self.add_item_to_inventory(inv_item.clone());
            self.push_log_tr("game.inv_full");
            return false;
        }
        self.ground_items.insert(key, inv_item.kind);
        self.push_log_trf(
            "game.swapped_ground",
            &[
                ("picked", crate::log_arg_item_ref(ground_item)),
                ("placed", crate::log_arg_item_ref(inv_item.kind)),
            ],
        );
        true
    }

    pub(crate) fn use_inventory_item(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            self.push_log_tr("game.no_usable");
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
                self.player_hunger =
                    (self.player_hunger + ITEM_USE_HUNGER_RESTORE).min(self.player_max_hunger);
                let healed = self.player_hp - before;
                if healed > 0 {
                    self.push_log_trf(
                        "game.used_heal",
                        &[
                            ("item", crate::log_arg_inventory_item_ref(&item)),
                            ("heal", healed.to_string()),
                            ("hp", self.player_hp.to_string()),
                            ("max", self.player_max_hp.to_string()),
                        ],
                    );
                } else {
                    self.push_log_trf(
                        "game.used_no_heal",
                        &[("item", crate::log_arg_inventory_item_ref(&item))],
                    );
                }
                true
            }
            Item::Herb => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let before = self.player_hp;
                self.player_hp = (self.player_hp + 3).min(self.player_max_hp);
                self.player_hunger =
                    (self.player_hunger + ITEM_USE_HUNGER_RESTORE).min(self.player_max_hunger);
                let healed = self.player_hp - before;
                if healed > 0 {
                    self.push_log_trf(
                        "game.used_heal",
                        &[
                            ("item", crate::log_arg_inventory_item_ref(&item)),
                            ("heal", healed.to_string()),
                            ("hp", self.player_hp.to_string()),
                            ("max", self.player_max_hp.to_string()),
                        ],
                    );
                } else {
                    self.push_log_trf(
                        "game.used_no_heal",
                        &[("item", crate::log_arg_inventory_item_ref(&item))],
                    );
                }
                true
            }
            Item::Elixir => {
                let Some(item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let before = self.player_hp;
                self.player_hp = (self.player_hp + 12).min(self.player_max_hp);
                self.player_hunger =
                    (self.player_hunger + ITEM_USE_HUNGER_RESTORE).min(self.player_max_hunger);
                let healed = self.player_hp - before;
                if healed > 0 {
                    self.push_log_trf(
                        "game.used_heal",
                        &[
                            ("item", crate::log_arg_inventory_item_ref(&item)),
                            ("heal", healed.to_string()),
                            ("hp", self.player_hp.to_string()),
                            ("max", self.player_max_hp.to_string()),
                        ],
                    );
                } else {
                    self.push_log_trf(
                        "game.used_no_heal",
                        &[("item", crate::log_arg_inventory_item_ref(&item))],
                    );
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
                    self.push_log_trf(
                        "game.used_hunger",
                        &[
                            ("item", crate::log_arg_inventory_item_ref(&item)),
                            ("v", restored.to_string()),
                            ("cur", self.player_hunger.to_string()),
                            ("max", self.player_max_hunger.to_string()),
                        ],
                    );
                } else {
                    self.push_log_trf(
                        "game.used_no_hunger",
                        &[("item", crate::log_arg_inventory_item_ref(&item))],
                    );
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
                    self.push_log_trf(
                        "game.used_hunger",
                        &[
                            ("item", crate::log_arg_inventory_item_ref(&item)),
                            ("v", restored.to_string()),
                            ("cur", self.player_hunger.to_string()),
                            ("max", self.player_max_hunger.to_string()),
                        ],
                    );
                } else {
                    self.push_log_trf(
                        "game.used_no_hunger",
                        &[("item", crate::log_arg_inventory_item_ref(&item))],
                    );
                }
                true
            }
            Item::Torch => {
                let p = (self.player.x, self.player.y);
                if self.torches.contains(&p) {
                    self.push_log_tr("game.torch_already");
                    return false;
                }
                let Some(_item) = self.take_inventory_one(idx) else {
                    return false;
                };
                self.torches.insert(p);
                self.push_log_tr("game.torch_placed");
                true
            }
            Item::FlameScroll => {
                if !self.try_spend_mp(FLAME_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_flame_scroll();
                true
            }
            Item::EmberScroll => {
                if !self.try_spend_mp(EMBER_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_line_tech(5, 2, 4, 0, true);
                true
            }
            Item::BlinkScroll => {
                if !self.try_spend_mp(BLINK_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_blink_scroll();
                true
            }
            Item::BindScroll => {
                if !self.try_spend_mp(BIND_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_line_tech(5, 1, 0, 4, false);
                true
            }
            Item::RepulseScroll => {
                if !self.try_spend_mp(REPULSE_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_repulse_scroll();
                true
            }
            Item::NovaScroll => {
                if !self.try_spend_mp(NOVA_SCROLL_MP_COST) {
                    return false;
                }
                let _ = self.cast_nova_scroll();
                true
            }
            Item::PulseBomb => {
                let Some(_item) = self.take_inventory_one(idx) else {
                    return false;
                };
                let _ = self.cast_burst_tech(1, 3, 0, 3, 1);
                true
            }
            Item::ForgeScroll => self.try_cast_forge_scroll(),
            Item::StoneAxe
            | Item::IronSword
            | Item::IronPickaxe
            | Item::GladiusNadir
            | Item::FerrumOccasus
            | Item::VirgaOriens
            | Item::VirgaMeridies
            | Item::VirgaZenith => {
                let item = self.inventory[idx].clone();
                let equipped_name = crate::log_arg_inventory_item_ref(&item);
                let is_same = self
                    .equipped_sword
                    .as_ref()
                    .is_some_and(|eq| eq.same_identity(&item));
                if is_same && Self::is_ancient_weapon_item(kind) {
                    self.try_cast_ancient_weapon_art(kind)
                } else if is_same {
                    self.equipped_sword = None;
                    self.push_log_trf("game.unequipped_item", &[("item", equipped_name)]);
                    true
                } else {
                    self.equipped_sword = Some(item);
                    self.push_log_trf("game.equipped_item", &[("item", equipped_name)]);
                    true
                }
            }
            Item::WoodenShield => {
                let item = self.inventory[idx].clone();
                let equipped_name = crate::log_arg_inventory_item_ref(&item);
                let is_same = self
                    .equipped_shield
                    .as_ref()
                    .is_some_and(|eq| eq.same_identity(&item));
                if is_same {
                    self.equipped_shield = None;
                    self.push_log_trf("game.unequipped_item", &[("item", equipped_name)]);
                } else {
                    self.equipped_shield = Some(item);
                    self.push_log_trf(
                        "game.equipped_slot",
                        &[
                            ("item", equipped_name),
                            ("slot", crate::log_arg_text_ref("status.slot.shield")),
                        ],
                    );
                }
                true
            }
            Item::LuckyCharm => {
                let item = self.inventory[idx].clone();
                let equipped_name = crate::log_arg_inventory_item_ref(&item);
                let is_same = self
                    .equipped_accessory
                    .as_ref()
                    .is_some_and(|eq| eq.same_identity(&item));
                if is_same {
                    self.equipped_accessory = None;
                    self.push_log_trf("game.unequipped_item", &[("item", equipped_name)]);
                } else {
                    self.equipped_accessory = Some(item);
                    self.push_log_trf(
                        "game.equipped_slot",
                        &[
                            ("item", equipped_name),
                            ("slot", crate::log_arg_text_ref("status.slot.accessory")),
                        ],
                    );
                }
                true
            }
            Item::Wood
            | Item::Stone
            | Item::StringFiber
            | Item::IronIngot
            | Item::Hide
            | Item::QuartzMemoryKnowledge
            | Item::QuartzMemoryLife
            | Item::QuartzMemoryDimension
            | Item::QuartzMemoryInterface
            | Item::QuartzMemoryExtraction
            | Item::QuartzMemoryArchive
            | Item::QuartzMemoryCathedral
            | Item::QuartzMemoryHalo
            | Item::QuartzMemoryLung
            | Item::QuartzMemoryOssuary
            | Item::QuartzMemoryChoir
            | Item::QuartzMemoryWitness => {
                self.push_log_tr("game.cannot_use_direct");
                false
            }
        }
    }

    fn try_spend_mp(&mut self, cost: i32) -> bool {
        if self.player_mp < cost {
            self.push_log_trf(
                "game.no_mp",
                &[
                    ("need", cost.to_string()),
                    ("mp", self.player_mp.max(0).to_string()),
                ],
            );
            return false;
        }
        self.player_mp = self.player_mp.saturating_sub(cost);
        true
    }

    fn is_ancient_weapon_item(item: Item) -> bool {
        matches!(
            item,
            Item::GladiusNadir
                | Item::FerrumOccasus
                | Item::VirgaOriens
                | Item::VirgaMeridies
                | Item::VirgaZenith
        )
    }

    fn ancient_weapon_cost(item: Item) -> Option<u8> {
        match item {
            Item::GladiusNadir => Some(2),
            Item::FerrumOccasus => Some(1),
            Item::VirgaOriens => Some(1),
            Item::VirgaMeridies => Some(1),
            Item::VirgaZenith => Some(2),
            _ => None,
        }
    }

    fn can_spend_ancient_for_weapon(&self, item: Item) -> bool {
        let Some(cost) = Self::ancient_weapon_cost(item) else {
            return false;
        };
        self.ancient_charge >= cost
    }

    fn spend_ancient_for_weapon(&mut self, item: Item) -> bool {
        let Some(cost) = Self::ancient_weapon_cost(item) else {
            return false;
        };
        if self.ancient_charge < cost {
            self.push_log_tr("ancient.weapon_silent");
            return false;
        }
        self.ancient_charge = self.ancient_charge.saturating_sub(cost);
        true
    }

    fn item_attack_bonus(item: Item) -> i32 {
        match item {
            Item::StoneAxe => 2,
            Item::IronPickaxe | Item::IronSword => 3,
            Item::GladiusNadir => 5,
            Item::FerrumOccasus | Item::VirgaOriens => 4,
            Item::VirgaMeridies => 3,
            Item::VirgaZenith => 2,
            _ => 0,
        }
    }

    fn is_line_aligned(dx: i32, dy: i32) -> bool {
        dx == 0 || dy == 0 || dx.abs() == dy.abs()
    }

    fn cast_piercing_line_tech(
        &mut self,
        max_range: i32,
        damage_bonus: i32,
        burning_turns: u8,
        slowed_turns: u8,
    ) -> bool {
        let (dx, dy) = self.facing.delta();
        let mut target_ids: Vec<u64> = Vec::new();
        let mut reach = 0;
        for step in 1..=max_range {
            let tx = self.player.x + dx * step;
            let ty = self.player.y + dy * step;
            if !self.tile(tx, ty).walkable() || self.has_blocking_structure_at(tx, ty) {
                break;
            }
            reach = step;
            if let Some(enemy) = self.enemies.iter().find(|e| e.pos.x == tx && e.pos.y == ty) {
                target_ids.push(enemy.id);
            }
        }
        if target_ids.is_empty() {
            self.push_log_tr("game.no_target");
            return false;
        }
        self.push_flame_line_effect(
            self.player,
            self.facing,
            reach.max(1) as u8,
            0,
            "player",
        );
        for enemy_id in target_ids {
            let Some(i) = self.enemies.iter().position(|e| e.id == enemy_id) else {
                continue;
            };
            let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
            let damage = calc_damage(self.player_attack_power() + damage_bonus, enemy_def);
            self.stat_damage_dealt = self.stat_damage_dealt.saturating_add(damage as u32);
            self.enemies[i].hp -= damage;
            if Self::is_traveler_id(&self.enemies[i].creature_id) {
                self.enemies[i].flee_from_player = true;
            }
            self.apply_enemy_statuses(i, burning_turns, slowed_turns);
            if self.enemies[i].hp <= 0 {
                self.defeat_enemy_at(i);
            } else {
                self.push_log_trf(
                    "game.you_hit_enemy",
                    &[
                        ("enemy", crate::log_arg_creature_ref(&self.enemies[i].creature_id)),
                        ("damage", damage.to_string()),
                    ],
                );
            }
        }
        true
    }

    fn cast_zenith_transit(&mut self) -> bool {
        let (dx, dy) = self.facing.delta();
        let mut crossed_barrier = false;
        let mut fallback = self.player;
        let mut destination: Option<Pos> = None;
        for step in 1..=6 {
            let tx = self.player.x + dx * step;
            let ty = self.player.y + dy * step;
            let blocked = !self.tile(tx, ty).walkable() || self.has_blocking_structure_at(tx, ty);
            if blocked {
                crossed_barrier = true;
                continue;
            }
            if self.has_enemy_at(tx, ty) {
                continue;
            }
            if crossed_barrier {
                destination = Some(Pos { x: tx, y: ty });
                break;
            }
            if step <= 3 {
                fallback = Pos { x: tx, y: ty };
            }
        }
        let Some(dest) = destination.or_else(|| {
            if fallback.x != self.player.x || fallback.y != self.player.y {
                Some(fallback)
            } else {
                None
            }
        }) else {
            self.push_log_tr("ancient.zenith_fail");
            return false;
        };
        self.player = dest;
        self.pick_up_item_at_player();
        self.push_log_trf(
            "ancient.zenith_step",
            &[("x", dest.x.to_string()), ("y", dest.y.to_string())],
        );
        true
    }

    fn try_cast_ancient_weapon_art(&mut self, item: Item) -> bool {
        let can_pay = self.can_spend_ancient_for_weapon(item);
        let casted = match item {
            Item::GladiusNadir => can_pay && self.cast_piercing_line_tech(5, 4, 0, 1),
            Item::FerrumOccasus => can_pay && self.cast_burst_tech(1, 2, 0, 2, 2),
            Item::VirgaOriens => can_pay && self.cast_line_tech(6, 3, 4, 0, true),
            Item::VirgaMeridies => can_pay && self.cast_line_tech(5, 2, 0, 5, false),
            Item::VirgaZenith => can_pay && self.cast_zenith_transit(),
            _ => false,
        };
        if !casted {
            if !can_pay {
                self.push_log_tr("ancient.weapon_silent");
            }
            return false;
        }
        let _ = self.spend_ancient_for_weapon(item);
        self.push_log_trf("ancient.weapon_answer", &[("item", crate::log_arg_item_ref(item))]);
        true
    }

    fn enemy_pick_up_ground_item(&mut self, idx: usize) {
        let pos = self.enemies[idx].pos;
        let Some(item) = self.ground_items.get(&(pos.x, pos.y)).copied() else {
            return;
        };
        if Self::is_weapon_item(item) {
            self.enemies[idx].equipped_weapon = Some(item);
        }
        self.enemies[idx].carried_items.push(CarriedItem {
            item,
            drop_chance: 100,
        });
        self.ground_items.remove(&(pos.x, pos.y));
        if (self.player.x - pos.x).abs().max((self.player.y - pos.y).abs()) <= crate::VISION_RADIUS {
            self.push_log_trf(
                "game.enemy_pickup_item",
                &[
                    ("enemy", crate::log_arg_creature_ref(&self.enemies[idx].creature_id)),
                    ("item", crate::log_arg_item_ref(item)),
                ],
            );
        }
    }

    fn try_enemy_builtin_special(
        &mut self,
        idx: usize,
        chebyshev: i32,
        dx: i32,
        dy: i32,
        attack_order: &mut u16,
    ) -> bool {
        let current = self.enemies[idx].pos;
        let research_score = self.research_structure_score(current.x, current.y);
        let special = match self.enemies[idx].creature_id.as_str() {
            "relay_surgeon" if (self.turn + self.enemies[idx].id).is_multiple_of(3) => {
                Some((4, 3, 0, true))
            }
            "archive_scribe" if (self.turn + self.enemies[idx].id + 1).is_multiple_of(3) => {
                Some((3, 0, 3, false))
            }
            "carrier_frame"
                if (2..=4).contains(&chebyshev)
                    && Self::is_line_aligned(dx, dy)
                    && (self.turn + self.enemies[idx].id).is_multiple_of(2) =>
            {
                Some((5, 0, 2, false))
            }
            "specimen_guard"
                if research_score >= 3
                    && (2..=4).contains(&chebyshev)
                    && Self::is_line_aligned(dx, dy)
                    && (self.turn + self.enemies[idx].id + 1).is_multiple_of(3) =>
            {
                Some((4, 0, 3, false))
            }
            _ => None,
        };
        let Some((damage, burning_turns, slowed_turns, flame_visual)) = special else {
            return false;
        };
        let delay_u16 = (ENEMY_ATTACK_BASE_DELAY_FRAMES as u16)
            + *attack_order * (ENEMY_ATTACK_STAGGER_FRAMES as u16);
        let delay_frames = delay_u16.min(u8::MAX as u16) as u8;
        *attack_order = attack_order.saturating_add(1);
        if let Some(facing) = Facing::from_delta(dx.signum(), dy.signum()) {
            self.enemies[idx].facing = facing;
        }
        if flame_visual {
            self.push_flame_line_effect(
                current,
                self.enemies[idx].facing,
                chebyshev as u8,
                delay_frames,
                "enemy",
            );
        } else {
            self.push_attack_effect(current, self.player, delay_frames);
        }
        self.pending_enemy_hits.push(PendingEnemyHit {
            enemy_name: crate::log_arg_creature_ref(&self.enemies[idx].creature_id),
            damage,
            burning_turns,
            slowed_turns,
            delay_frames,
            attacker_pos: current,
            attacker_agility: self.enemy_agility(idx),
        });
        true
    }

    fn enemy_try_ancient_weapon_art(&mut self, idx: usize, chebyshev: i32, dx: i32, dy: i32) -> bool {
        let Some(item) = self.enemies[idx].equipped_weapon else {
            return false;
        };
        let current = self.enemies[idx].pos;
        let delay_frames = ENEMY_ATTACK_BASE_DELAY_FRAMES;
        match item {
            Item::GladiusNadir if (2..=5).contains(&chebyshev) && Self::is_line_aligned(dx, dy) => {
                if let Some(facing) = Facing::from_delta(dx.signum(), dy.signum()) {
                    self.enemies[idx].facing = facing;
                }
                self.push_flame_line_effect(current, self.enemies[idx].facing, chebyshev as u8, delay_frames, "enemy");
                self.pending_enemy_hits.push(PendingEnemyHit {
                    enemy_name: crate::log_arg_creature_ref(&self.enemies[idx].creature_id),
                    damage: 5,
                    burning_turns: 0,
                    slowed_turns: 2,
                    delay_frames,
                    attacker_pos: current,
                    attacker_agility: self.enemy_agility(idx),
                });
                true
            }
            Item::FerrumOccasus if chebyshev == 1 => {
                self.push_visual_effect("nova_burst", current, self.enemies[idx].facing, "enemy", 0, None);
                self.pending_enemy_hits.push(PendingEnemyHit {
                    enemy_name: crate::log_arg_creature_ref(&self.enemies[idx].creature_id),
                    damage: 4,
                    burning_turns: 0,
                    slowed_turns: 2,
                    delay_frames: 0,
                    attacker_pos: current,
                    attacker_agility: self.enemy_agility(idx),
                });
                true
            }
            Item::VirgaOriens if (2..=6).contains(&chebyshev) && Self::is_line_aligned(dx, dy) => {
                if let Some(facing) = Facing::from_delta(dx.signum(), dy.signum()) {
                    self.enemies[idx].facing = facing;
                }
                self.push_flame_line_effect(current, self.enemies[idx].facing, chebyshev as u8, delay_frames, "enemy");
                self.pending_enemy_hits.push(PendingEnemyHit {
                    enemy_name: crate::log_arg_creature_ref(&self.enemies[idx].creature_id),
                    damage: 4,
                    burning_turns: 4,
                    slowed_turns: 0,
                    delay_frames,
                    attacker_pos: current,
                    attacker_agility: self.enemy_agility(idx),
                });
                true
            }
            Item::VirgaMeridies if (2..=5).contains(&chebyshev) && Self::is_line_aligned(dx, dy) => {
                if let Some(facing) = Facing::from_delta(dx.signum(), dy.signum()) {
                    self.enemies[idx].facing = facing;
                }
                self.push_attack_effect(current, self.player, delay_frames);
                self.pending_enemy_hits.push(PendingEnemyHit {
                    enemy_name: crate::log_arg_creature_ref(&self.enemies[idx].creature_id),
                    damage: 3,
                    burning_turns: 0,
                    slowed_turns: 5,
                    delay_frames,
                    attacker_pos: current,
                    attacker_agility: self.enemy_agility(idx),
                });
                true
            }
            Item::VirgaZenith if (2..=6).contains(&chebyshev) => {
                let mut next = current;
                if let Some(facing) = Facing::from_delta(dx.signum(), dy.signum()) {
                    self.enemies[idx].facing = facing;
                }
                let (mx, my) = self.enemies[idx].facing.delta();
                for _ in 0..2 {
                    let tx = next.x + mx;
                    let ty = next.y + my;
                    if !self.tile(tx, ty).walkable()
                        || self.has_blocking_structure_at(tx, ty)
                        || self.has_enemy_at(tx, ty)
                        || (tx == self.player.x && ty == self.player.y)
                    {
                        break;
                    }
                    next = Pos { x: tx, y: ty };
                }
                if next.x != current.x || next.y != current.y {
                    self.enemies[idx].pos = next;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn cast_line_tech(
        &mut self,
        max_range: i32,
        damage_bonus: i32,
        burning_turns: u8,
        slowed_turns: u8,
        flame_visual: bool,
    ) -> bool {
        let (dx, dy) = self.facing.delta();
        let mut max_reach_step: i32 = 0;
        for step in 1..=max_range {
            let tx = self.player.x + dx * step;
            let ty = self.player.y + dy * step;
            if !self.tile(tx, ty).walkable() || self.has_blocking_structure_at(tx, ty) {
                break;
            }
            max_reach_step = step;
            if let Some(i) = self
                .enemies
                .iter()
                .position(|e| e.pos.x == tx && e.pos.y == ty)
            {
                if flame_visual {
                    self.push_flame_line_effect(self.player, self.facing, step as u8, 0, "player");
                } else {
                    self.push_attack_effect(self.player, Pos { x: tx, y: ty }, 0);
                }
                let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
                let damage = calc_damage(self.player_attack_power() + damage_bonus, enemy_def);
                self.stat_damage_dealt = self.stat_damage_dealt.saturating_add(damage as u32);
                self.enemies[i].hp -= damage;
                if Self::is_traveler_id(&self.enemies[i].creature_id) {
                    self.enemies[i].flee_from_player = true;
                }
                self.apply_enemy_statuses(i, burning_turns, slowed_turns);
                if self.enemies[i].hp <= 0 {
                    self.defeat_enemy_at(i);
                } else {
                    self.push_log_trf(
                        "game.you_hit_enemy",
                        &[
                            ("enemy", crate::log_arg_creature_ref(&self.enemies[i].creature_id)),
                            ("damage", damage.to_string()),
                        ],
                    );
                }
                return true;
            }
        }
        if flame_visual {
            let length = if max_reach_step > 0 {
                max_reach_step as u8
            } else {
                1
            };
            self.push_flame_line_effect(self.player, self.facing, length, 0, "player");
        }
        self.push_log_tr("game.no_target");
        true
    }

    fn knockback_enemy_from(&mut self, idx: usize, origin: Pos, max_steps: i32) -> bool {
        let mut dx = self.enemies[idx].pos.x - origin.x;
        let mut dy = self.enemies[idx].pos.y - origin.y;
        dx = dx.signum();
        dy = dy.signum();
        if dx == 0 && dy == 0 {
            return false;
        }
        let mut dest = self.enemies[idx].pos;
        for _ in 0..max_steps.max(0) {
            let next = Pos {
                x: dest.x + dx,
                y: dest.y + dy,
            };
            if !self.tile(next.x, next.y).walkable()
                || self.has_blocking_structure_at(next.x, next.y)
                || (next.x == self.player.x && next.y == self.player.y)
                || self
                    .enemies
                    .iter()
                    .enumerate()
                    .any(|(j, e)| j != idx && e.pos.x == next.x && e.pos.y == next.y)
            {
                break;
            }
            dest = next;
        }
        if dest.x == self.enemies[idx].pos.x && dest.y == self.enemies[idx].pos.y {
            return false;
        }
        self.enemies[idx].pos = dest;
        true
    }

    fn cast_burst_tech(
        &mut self,
        radius: i32,
        damage_bonus: i32,
        burning_turns: u8,
        slowed_turns: u8,
        knockback_steps: i32,
    ) -> bool {
        let radius2 = radius * radius;
        let target_ids: Vec<u64> = self
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
        if target_ids.is_empty() {
            self.push_log_tr("game.no_target");
            return false;
        }
        self.push_visual_effect("nova_burst", self.player, self.facing, "player", 0, None);
        for enemy_id in target_ids {
            let Some(i) = self.enemies.iter().position(|e| e.id == enemy_id) else {
                continue;
            };
            let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
            let damage = calc_damage(self.player_attack_power() + damage_bonus, enemy_def);
            self.stat_damage_dealt = self.stat_damage_dealt.saturating_add(damage as u32);
            self.enemies[i].hp -= damage;
            self.apply_enemy_statuses(i, burning_turns, slowed_turns);
            let _ = self.knockback_enemy_from(i, self.player, knockback_steps);
            if self.enemies[i].hp <= 0 {
                self.defeat_enemy_at(i);
            } else {
                self.push_log_trf(
                    "game.you_hit_enemy",
                    &[
                        ("enemy", crate::log_arg_creature_ref(&self.enemies[i].creature_id)),
                        ("damage", damage.to_string()),
                    ],
                );
            }
        }
        true
    }

    fn thrown_attack_profile(kind: Item) -> Option<(i32, u8, u8)> {
        match kind {
            Item::Stone => Some((4, 0, 0)),
            Item::IronIngot => Some((5, 0, 0)),
            Item::Torch => Some((3, 2, 0)),
            Item::StoneAxe
            | Item::IronSword
            | Item::IronPickaxe
            | Item::GladiusNadir
            | Item::FerrumOccasus
            | Item::VirgaOriens
            | Item::VirgaMeridies
            | Item::VirgaZenith => Some((6, 0, 0)),
            Item::EmberScroll => Some((2, 3, 0)),
            Item::BindScroll => Some((2, 0, 3)),
            _ => None,
        }
    }

    fn cast_flame_scroll(&mut self) -> bool {
        self.cast_line_tech(6, 4, 0, 0, true)
    }

    fn cast_repulse_scroll(&mut self) -> bool {
        self.cast_burst_tech(2, 1, 0, 2, 2)
    }

    fn cast_blink_scroll(&mut self) -> bool {
        let (fx, fy) = self.facing.delta();
        let mut tx = self.player.x;
        let mut ty = self.player.y;
        for _ in 0..4 {
            let nx = tx + fx;
            let ny = ty + fy;
            if !self.tile(nx, ny).walkable()
                || self.has_blocking_structure_at(nx, ny)
                || self.has_enemy_at(nx, ny)
            {
                break;
            }
            tx = nx;
            ty = ny;
        }
        if (tx, ty) == (self.player.x, self.player.y) {
            self.push_log_tr("game.no_target");
            return false;
        }
        self.player = Pos { x: tx, y: ty };
        self.pick_up_item_at_player();
        self.push_log_trf(
            "game.blink_to",
            &[("x", tx.to_string()), ("y", ty.to_string())],
        );
        true
    }

    fn cast_nova_scroll(&mut self) -> bool {
        const NOVA_RADIUS: i32 = 4;
        let radius2 = NOVA_RADIUS * NOVA_RADIUS;
        self.push_visual_effect("nova_burst", self.player, self.facing, "player", 0, None);

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
            self.push_log_tr("game.no_target");
            return true;
        }

        for enemy_id in target_ids {
            let Some(i) = self.enemies.iter().position(|e| e.id == enemy_id) else {
                continue;
            };
            let enemy_pos = self.enemies[i].pos;
            self.push_attack_effect(self.player, enemy_pos, 0);
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
                self.maybe_complete_substory_facility_by_guardian(dead.id);
                self.blood_stains.insert((dead.pos.x, dead.pos.y));
                self.push_death_cry(&dead.creature_id);
                self.push_log_trf(
                    "game.you_defeated",
                    &[("enemy", crate::log_arg_creature_ref(&dead.creature_id))],
                );
                if Self::is_traveler_id(&dead.creature_id) {
                    self.maybe_drop_traveler_bread(dead.pos);
                } else {
                    self.maybe_drop_enemy_carried_item(&dead);
                }
            } else {
                self.push_log_trf(
                    "game.you_hit_enemy",
                    &[
                        ("enemy", crate::log_arg_creature_ref(&self.enemies[i].creature_id)),
                        ("damage", damage.to_string()),
                    ],
                );
            }
        }
        true
    }

    fn try_cast_forge_scroll(&mut self) -> bool {
        let Some(equipped) = self.equipped_sword.as_ref() else {
            self.push_log_tr("game.ritual_need_weapon");
            return false;
        };
        if !self.has_blood_stain(self.player.x, self.player.y) {
            self.push_log_tr("game.ritual_need_blood");
            return false;
        }
        if equipped.weapon_bonus >= FORGE_SCROLL_ATK_BONUS_MAX {
            self.push_log_tr("game.ritual_weapon_max");
            return false;
        }

        let pattern = crate::defs::forge_scroll_pattern();
        for &(dx, dy, expected) in pattern {
            let tx = self.player.x + dx;
            let ty = self.player.y + dy;
            if self.ground_items.get(&(tx, ty)).copied() != Some(expected) {
                self.push_log_tr("game.ritual_pattern_mismatch");
                return false;
            }
        }

        if !self.try_spend_mp(FORGE_SCROLL_MP_COST) {
            return false;
        }

        for &(dx, dy, _) in pattern {
            let tx = self.player.x + dx;
            let ty = self.player.y + dy;
            let delay = (dx.abs().max(dy.abs()) as u8).saturating_mul(2);
            self.push_visual_effect(
                "ritual_flame",
                Pos { x: tx, y: ty },
                Facing::N,
                "player",
                delay,
                None,
            );
            self.ground_items.remove(&(tx, ty));
        }
        let mut current_bonus = 0;
        if let Some(eq) = self.equipped_sword.as_mut() {
            eq.weapon_bonus =
                (eq.weapon_bonus + FORGE_SCROLL_ATK_BONUS_GAIN).min(FORGE_SCROLL_ATK_BONUS_MAX);
            current_bonus = eq.weapon_bonus;
            let equipped_copy = eq.clone();
            if let Some(inv_item) = self
                .inventory
                .iter_mut()
                .find(|it| it.same_identity(&equipped_copy))
            {
                inv_item.weapon_bonus = eq.weapon_bonus;
            }
        }
        self.push_log_trf(
            "game.ritual_weapon_up",
            &[
                ("add", FORGE_SCROLL_ATK_BONUS_GAIN.to_string()),
                ("cur", current_bonus.to_string()),
                ("max", FORGE_SCROLL_ATK_BONUS_MAX.to_string()),
            ],
        );
        true
    }

    fn maybe_drop_traveler_bread(&mut self, pos: Pos) {
        if self.rand_u32() % 100 < 40 {
            let _ = self.place_ground_item_near(pos.x, pos.y, Item::Bread);
            self.push_log_trf(
                "game.enemy_drop_item",
                &[("item", crate::log_arg_item_ref(Item::Bread))],
            );
        }
    }

    pub(crate) fn drop_inventory_item(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            return false;
        }
        let key = (self.player.x, self.player.y);
        if self.ground_items.contains_key(&key) {
            self.push_log_tr("game.cannot_drop_here");
            return false;
        }
        let Some(item) = self.take_inventory_one(idx) else {
            return false;
        };
        self.ground_items.insert(key, item.kind);
        self.push_log_trf("game.dropped", &[("item", crate::log_arg_inventory_item_ref(&item))]);
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
        let profile = Self::thrown_attack_profile(item.kind);
        for _ in 0..3 {
            let nx = tx + fx;
            let ny = ty + fy;
            if !self.tile(nx, ny).walkable() || self.has_blocking_structure_at(nx, ny) {
                break;
            }
            if let Some(i) = self.enemies.iter().position(|e| e.pos.x == nx && e.pos.y == ny) {
                tx = nx;
                ty = ny;
                self.push_attack_effect(self.player, Pos { x: tx, y: ty }, 0);
                if let Some((damage_bonus, burning_turns, slowed_turns)) = profile {
                    let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
                    let damage = calc_damage(self.player_attack_power() + damage_bonus, enemy_def);
                    self.stat_damage_dealt = self.stat_damage_dealt.saturating_add(damage as u32);
                    self.enemies[i].hp -= damage;
                    self.apply_enemy_statuses(i, burning_turns, slowed_turns);
                    if self.enemies[i].hp <= 0 {
                        self.defeat_enemy_at(i);
                    } else {
                        self.push_log_trf(
                            "game.you_hit_enemy",
                            &[
                                ("enemy", crate::log_arg_creature_ref(&self.enemies[i].creature_id)),
                                ("damage", damage.to_string()),
                            ],
                        );
                    }
                }
                let _ = self.place_ground_item_near(tx, ty, item.kind);
                return true;
            }
            tx = nx;
            ty = ny;
        }
        if (tx, ty) == (self.player.x, self.player.y) {
            self.push_log_trf(
                "game.throw_feet",
                &[("item", crate::log_arg_inventory_item_ref(&item))],
            );
        } else {
            self.push_log_trf(
                "game.throw_to",
                &[("item", crate::log_arg_inventory_item_ref(&item))],
            );
        }
        if !self.place_ground_item_near(tx, ty, item.kind) {
            self.push_log_trf(
                "game.lost_item",
                &[("item", crate::log_arg_inventory_item_ref(&item))],
            );
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
            self.push_log_tr("game.rename_reset");
        } else {
            self.inventory[idx].custom_name = Some(trimmed.clone());
            self.push_log_trf("game.rename_to", &[("name", trimmed)]);
        }
        true
    }

    fn consume_turn(&mut self) {
        self.tick_enemies();
        self.turn = self.turn.saturating_add(1);
        if self.player_status.burning_turns > 0 && self.player_hp > 0 {
            if !self.invincible {
                self.player_hp = self.player_hp.saturating_sub(1);
                self.stat_damage_taken = self.stat_damage_taken.saturating_add(1);
                self.push_log_tr("game.burning_tick_you");
                if self.player_hp <= 0 && self.death_cause.is_none() {
                    self.death_cause = Some(tr("death.cause.burning").to_string());
                }
            }
            self.player_status.burning_turns = self.player_status.burning_turns.saturating_sub(1);
        }
        if self.player_status.slowed_turns > 0 {
            self.player_status.slowed_turns = self.player_status.slowed_turns.saturating_sub(1);
        }
        let mut idx = 0usize;
        while idx < self.enemies.len() {
            if self.enemies[idx].status.burning_turns > 0 {
                self.enemies[idx].status.burning_turns =
                    self.enemies[idx].status.burning_turns.saturating_sub(1);
                self.enemies[idx].hp = self.enemies[idx].hp.saturating_sub(1);
                if self.enemies[idx].hp <= 0 {
                    self.push_log_trf(
                        "game.burning_tick_enemy",
                        &[("enemy", crate::log_arg_creature_ref(&self.enemies[idx].creature_id))],
                    );
                    self.defeat_enemy_at(idx);
                    continue;
                }
            }
            if self.enemies[idx].status.slowed_turns > 0 {
                self.enemies[idx].status.slowed_turns =
                    self.enemies[idx].status.slowed_turns.saturating_sub(1);
            }
            idx += 1;
        }
        if !self.invincible && self.turn.is_multiple_of(4) {
            self.player_hunger = self.player_hunger.saturating_sub(1).max(0);
        }
        if !self.invincible && self.player_hunger <= 0 && self.player_hp > 0 {
            self.player_hp = self.player_hp.saturating_sub(1);
            if self.player_hp <= 0 && self.death_cause.is_none() {
                self.death_cause = Some(tr("death.cause.hunger").to_string());
            }
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
        self.push_log_trf("game.gain_exp", &[("exp", amount.to_string())]);
        while self.exp >= self.next_exp {
            self.exp -= self.next_exp;
            self.level = self.level.saturating_add(1);
            self.next_exp = exp_needed_for_level(self.level);
            self.player_max_hp = self.player_max_hp.saturating_add(3);
            self.player_max_mp = self.player_max_mp.saturating_add(LEVEL_UP_MP_GAIN);
            self.player_mp = (self.player_mp + LEVEL_UP_MP_GAIN).min(self.player_max_mp);
            self.push_log_trf(
                "game.level_up",
                &[
                    ("level", self.level.to_string()),
                    ("hp", self.player_hp.to_string()),
                    ("max", self.player_max_hp.to_string()),
                    ("mp", self.player_mp.to_string()),
                    ("mp_max", self.player_max_mp.to_string()),
                ],
            );
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
        self.tile(x, y).walkable()
            && !self.has_blocking_structure_at(x, y)
            && !occupied.contains_key(&(x, y))
    }

    fn has_line_of_sight(&mut self, from: Pos, to: Pos) -> bool {
        if from.x == to.x && from.y == to.y {
            return true;
        }
        let mut x = from.x;
        let mut y = from.y;
        let dx = (to.x - from.x).abs();
        let sx = if from.x < to.x { 1 } else { -1 };
        let dy = -(to.y - from.y).abs();
        let sy = if from.y < to.y { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x == to.x && y == to.y {
                break;
            }
            let e2 = err * 2;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
            if x == to.x && y == to.y {
                break;
            }
            if tile_blocks_sight(self.tile(x, y)) {
                return false;
            }
        }
        true
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
                let chebyshev_to_player =
                    (self.player.x - nx).abs().max((self.player.y - ny).abs());
                if chebyshev_to_player == 1 {
                    return Some(step);
                }
                queue.push_back((Pos { x: nx, y: ny }, Some(step), depth.saturating_add(1)));
            }
        }
        None
    }

    fn is_weapon_item(item: Item) -> bool {
        matches!(
            item,
            Item::StoneAxe
                | Item::IronSword
                | Item::IronPickaxe
                | Item::GladiusNadir
                | Item::FerrumOccasus
                | Item::VirgaOriens
                | Item::VirgaMeridies
                | Item::VirgaZenith
        )
    }

    fn roll_percent(&mut self, chance: u8) -> bool {
        if chance == 0 {
            return false;
        }
        self.rand_u32() % 100 < chance as u32
    }

    fn roll_enemy_loadout(&mut self, creature_id: &str) -> (Vec<CarriedItem>, Option<Item>) {
        let mut carried_items: Vec<CarriedItem> = Vec::new();
        let mut equipped_weapon: Option<Item> = None;
        let loot_entries = creature_meta(creature_id).loot.clone();
        for loot in loot_entries {
            if !self.roll_percent(loot.carry_chance) {
                continue;
            }
            carried_items.push(CarriedItem {
                item: loot.item,
                drop_chance: loot.drop_chance,
            });
            if equipped_weapon.is_none() && loot.equip_as_weapon {
                equipped_weapon = Some(loot.item);
            }
        }
        if equipped_weapon.is_none() {
            equipped_weapon = carried_items
                .iter()
                .find(|c| Self::is_weapon_item(c.item))
                .map(|c| c.item);
        }
        (carried_items, equipped_weapon)
    }

    fn spawn_enemy_instance(&mut self, x: i32, y: i32, creature_id: &str) -> Enemy {
        let (carried_items, equipped_weapon) = self.roll_enemy_loadout(creature_id);
        Enemy {
            id: self.alloc_entity_id(),
            pos: Pos { x, y },
            hp: creature_meta(creature_id).hp,
            creature_id: creature_id.to_string(),
            carried_items,
            equipped_weapon,
            status: StatusState::default(),
            facing: self.random_facing(),
            flee_from_player: false,
        }
    }

    fn research_structure_score(&self, x: i32, y: i32) -> u32 {
        let mut score = 0u32;
        for dy in -3i32..=3i32 {
            for dx in -3i32..=3i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if dx.abs().max(dy.abs()) > 3 {
                    continue;
                }
                let Some(kind) = self.structure_at(x + dx, y + dy) else {
                    continue;
                };
                score += match kind {
                    StructureKind::Terminal => 4,
                    StructureKind::VendingMachine => 4,
                    StructureKind::BoneRack => 3,
                    StructureKind::CablePylon => 3,
                    StructureKind::Altar => 1,
                    StructureKind::TempleCore => 2,
                    StructureKind::SubstoryCore => 4,
                };
            }
        }
        score.min(8)
    }

    fn ritual_structure_score(&self, x: i32, y: i32) -> u32 {
        let mut score = 0u32;
        for dy in -2i32..=2i32 {
            for dx in -2i32..=2i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if let Some(kind) = self.stone_tablet_at(x + dx, y + dy) {
                    score += match kind {
                        StoneTabletKind::Mercy
                        | StoneTabletKind::MercyLitany
                        | StoneTabletKind::MercyName
                        | StoneTabletKind::Might
                        | StoneTabletKind::MightWarning
                        | StoneTabletKind::MightName
                        | StoneTabletKind::Oracle => 2,
                        _ => 1,
                    };
                }
                if let Some(kind) = self.structure_at(x + dx, y + dy) {
                    score += match kind {
                        StructureKind::Altar => 3,
                        StructureKind::TempleCore => 4,
                        _ => 0,
                    };
                }
            }
        }
        score.min(8)
    }

    fn nearby_hostile_count(&self, x: i32, y: i32, radius: i32) -> usize {
        self.enemies
            .iter()
            .filter(|e| {
                creature_meta(&e.creature_id).faction == Faction::Hostile
                    && (e.pos.x != x || e.pos.y != y)
                    && (e.pos.x - x).abs().max((e.pos.y - y).abs()) <= radius
            })
            .count()
    }

    fn nearby_wall_count(&mut self, x: i32, y: i32) -> usize {
        let mut walls = 0usize;
        for (dx, dy) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            let tx = x + dx;
            let ty = y + dy;
            if !self.tile(tx, ty).walkable() || self.has_blocking_structure_at(tx, ty) {
                walls += 1;
            }
        }
        walls
    }

    fn enemy_melee_profile(&mut self, idx: usize) -> (i32, u8, u8) {
        let pos = self.enemies[idx].pos;
        let ritual_score = self.ritual_structure_score(pos.x, pos.y);
        let research_score = self.research_structure_score(pos.x, pos.y);
        let nearby_hostiles = self.nearby_hostile_count(pos.x, pos.y, 1);
        let nearby_walls = self.nearby_wall_count(pos.x, pos.y);
        match self.enemies[idx].creature_id.as_str() {
            "ash_eater" => (if nearby_hostiles >= 1 { 1 } else { 0 }, 0, 0),
            "prayer_remnant" => (if ritual_score >= 2 { 1 } else { 0 }, 0, 0),
            "stone_warden" => (
                if ritual_score >= 2 { 2 } else { 0 },
                0,
                if ritual_score >= 4 { 1 } else { 0 },
            ),
            "incense_shell" => (0, 1, 0),
            "coffin_bearer" => (
                if nearby_walls >= 4 { 1 } else { 0 },
                0,
                2,
            ),
            "seal_hound" => (
                if nearby_hostiles >= 1 { 1 } else { 0 },
                0,
                if nearby_walls >= 4 { 1 } else { 0 },
            ),
            "buried_ember" => (0, 2, 0),
            "specimen_guard" => (
                if research_score >= 3 { 2 } else { 0 },
                0,
                if research_score >= 4 { 1 } else { 0 },
            ),
            "flayed_specimen" => (0, 2, 2),
            _ => (0, 0, 0),
        }
    }

    fn maybe_drop_enemy_carried_item(&mut self, dead: &Enemy) {
        if dead.carried_items.is_empty() {
            return;
        }
        let idx = (self.rand_u32() as usize) % dead.carried_items.len();
        let picked = dead.carried_items[idx];
        if !self.roll_percent(picked.drop_chance) {
            return;
        }
        let _ = self.place_ground_item_near(dead.pos.x, dead.pos.y, picked.item);
        self.push_log_trf(
            "game.enemy_drop_item",
            &[("item", crate::log_arg_item_ref(picked.item))],
        );
    }

    fn enemy_copper_drop_disks(&mut self, creature_id: &str) -> Option<u32> {
        let (chance, min_disks, span) = match creature_id {
            "scavenger_knight" => (55, 3, 4),
            "archive_scribe" => (48, 2, 3),
            "relay_surgeon" => (42, 2, 4),
            "specimen_guard" => (38, 2, 4),
            "dominion_vessel" => (24, 3, 5),
            "flayed_specimen" => (18, 1, 3),
            _ => return None,
        };
        if !self.roll_percent(chance) {
            return None;
        }
        Some(min_disks + (self.rand_u32() % span.max(1)) as u32)
    }

    fn maybe_drop_enemy_copper(&mut self, dead: &Enemy) {
        let Some(disks) = self.enemy_copper_drop_disks(&dead.creature_id) else {
            return;
        };
        if self.place_ground_copper_near(dead.pos.x, dead.pos.y, disks) {
            self.push_log_trf(
                "game.enemy_drop_copper",
                &[("grams", Self::copper_weight_text(disks))],
            );
        }
    }

    fn defeat_enemy_at(&mut self, idx: usize) {
        self.stat_enemies_defeated = self.stat_enemies_defeated.saturating_add(1);
        let dead = self.enemies.remove(idx);
        self.maybe_complete_substory_facility_by_guardian(dead.id);
        self.blood_stains.insert((dead.pos.x, dead.pos.y));
        self.push_death_cry(&dead.creature_id);
        self.push_log_trf(
            "game.you_defeated",
            &[("enemy", crate::log_arg_creature_ref(&dead.creature_id))],
        );
        let cdef = creature_meta(&dead.creature_id);
        let gained = (cdef.hp.max(1) + cdef.attack + cdef.defense * 2).max(1) as u32;
        self.gain_exp(gained);
        if Self::is_traveler_id(&dead.creature_id) {
            self.maybe_drop_traveler_bread(dead.pos);
        } else {
            self.maybe_drop_enemy_carried_item(&dead);
            self.maybe_drop_enemy_copper(&dead);
        }
    }

    fn apply_enemy_statuses(&mut self, idx: usize, burning_turns: u8, slowed_turns: u8) {
        if burning_turns == 0 && slowed_turns == 0 {
            return;
        }
        if burning_turns > 0 && self.enemies[idx].status.burning_turns < burning_turns {
            self.push_log_trf(
                "game.afflict_burning",
                &[("target", crate::log_arg_creature_ref(&self.enemies[idx].creature_id))],
            );
        }
        if slowed_turns > 0 && self.enemies[idx].status.slowed_turns < slowed_turns {
            self.push_log_trf(
                "game.afflict_slowed",
                &[("target", crate::log_arg_creature_ref(&self.enemies[idx].creature_id))],
            );
        }
        apply_status_bundle(&mut self.enemies[idx].status, burning_turns, slowed_turns);
    }

    fn apply_player_statuses(&mut self, source_name: &str, burning_turns: u8, slowed_turns: u8) {
        if burning_turns > 0 && self.player_status.burning_turns < burning_turns {
            self.push_log_trf("game.enemy_inflict_burning", &[("enemy", source_name.to_string())]);
        }
        if slowed_turns > 0 && self.player_status.slowed_turns < slowed_turns {
            self.push_log_trf("game.enemy_inflict_slowed", &[("enemy", source_name.to_string())]);
        }
        apply_status_bundle(&mut self.player_status, burning_turns, slowed_turns);
    }

    fn choose_spawn_enemy_kind(&mut self, x: i32, y: i32) -> Option<String> {
        let biome = self.world.biome_id_at(x, y);
        let research_score = self.research_structure_score(x, y);
        let cfg_pool = crate::world_cfg::enemy_pool_for_floor(self.floor);
        let spawnables: Vec<(String, u32)> = if cfg_pool.is_empty() {
            defs()
                .creatures
                .iter()
                .filter_map(|(id, c)| {
                    if c.faction == Faction::Hostile
                        && c.spawn_weight > 0
                        && Self::enemy_allowed_on_floor(id, self.floor)
                    {
                        let mul = biome_enemy_multiplier(biome, id);
                        let local_mul = research_spawn_multiplier(research_score, id);
                        let adjusted = c
                            .spawn_weight
                            .saturating_mul(mul)
                            .saturating_div(100)
                            .saturating_mul(local_mul)
                            .saturating_div(100);
                        if adjusted > 0 {
                            Some((id.to_string(), adjusted))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            cfg_pool
                .iter()
                .filter_map(|(id, base_weight)| {
                    if !Self::enemy_allowed_on_floor(id, self.floor) {
                        return None;
                    }
                    let mul = biome_enemy_multiplier(biome, id);
                    let local_mul = research_spawn_multiplier(research_score, id);
                    let adjusted = base_weight
                        .saturating_mul(mul)
                        .saturating_div(100)
                        .saturating_mul(local_mul)
                        .saturating_div(100);
                    if adjusted > 0 {
                        Some((id.clone(), adjusted))
                    } else {
                        None
                    }
                })
                .collect()
        };
        if spawnables.is_empty() {
            return None;
        }
        let total_weight: u32 = spawnables.iter().map(|(_, w)| *w).sum();
        let mut r = self.rand_u32() % total_weight.max(1);
        let mut chosen = spawnables[0].0.clone();
        for (id, w) in &spawnables {
            if r < *w {
                chosen = id.clone();
                break;
            }
            r -= *w;
        }
        Some(chosen)
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

    fn is_dark_spawn_tile(&mut self, x: i32, y: i32) -> bool {
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
            .filter(|e| (e.pos.x - x).abs().max((e.pos.y - y).abs()) <= DARK_SPAWN_DENSITY_RADIUS)
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
                    || self.has_blocking_structure_at(x, y)
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
        let enemy = self.spawn_enemy_instance(pick.0, pick.1, &kind);
        self.enemies.push(enemy);
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
            if !self.tile(x, y).walkable()
                || self.has_blocking_structure_at(x, y)
                || self.has_enemy_at(x, y)
            {
                continue;
            }
            let Some(chosen) = self.choose_spawn_enemy_kind(x, y) else {
                return;
            };
            let enemy = self.spawn_enemy_instance(x, y, &chosen);
            self.enemies.push(enemy);
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
            if !self.tile(x, y).walkable()
                || self.has_blocking_structure_at(x, y)
                || self.has_enemy_at(x, y)
            {
                continue;
            }
            let enemy = self.spawn_enemy_instance(x, y, "traveler");
            self.enemies.push(enemy);
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
            let is_hostile =
                creature_meta(&self.enemies[i].creature_id).faction == Faction::Hostile;
            let is_traveler = Self::is_traveler_id(&self.enemies[i].creature_id);

            let dx = self.player.x - current.x;
            let dy = self.player.y - current.y;
            let dist2 = dx * dx + dy * dy;
            let chebyshev = dx.abs().max(dy.abs());
            let player_visible_to_enemy = dist2 <= crate::VISION_RADIUS * crate::VISION_RADIUS
                && self.has_line_of_sight(current, self.player);
            if is_hostile && player_visible_to_enemy && (2..=5).contains(&chebyshev) {
                if self.enemy_try_ancient_weapon_art(i, chebyshev, dx, dy) {
                    occupied.insert((self.enemies[i].pos.x, self.enemies[i].pos.y), i);
                    continue;
                }
                if self.try_enemy_builtin_special(i, chebyshev, dx, dy, &mut attack_order) {
                    occupied.insert((current.x, current.y), i);
                    continue;
                }
            }
            if is_hostile && chebyshev == 1 {
                let (damage_bonus, burning_turns, slowed_turns) = self.enemy_melee_profile(i);
                let enemy_atk = creature_meta(&self.enemies[i].creature_id).attack
                    + damage_bonus
                    + self.enemies[i]
                        .equipped_weapon
                        .map(Self::item_attack_bonus)
                        .unwrap_or(0);
                let damage = calc_damage(enemy_atk, self.player_defense());
                let delay_u16 = (ENEMY_ATTACK_BASE_DELAY_FRAMES as u16)
                    + attack_order * (ENEMY_ATTACK_STAGGER_FRAMES as u16);
                let delay_frames = delay_u16.min(u8::MAX as u16) as u8;
                attack_order = attack_order.saturating_add(1);
                self.push_attack_effect(current, self.player, delay_frames);
                self.pending_enemy_hits.push(PendingEnemyHit {
                    enemy_name: crate::log_arg_creature_ref(&self.enemies[i].creature_id),
                    damage,
                    burning_turns,
                    slowed_turns,
                    delay_frames,
                    attacker_pos: current,
                    attacker_agility: self.enemy_agility(i),
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
                if !player_visible_to_enemy {
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
                if dist2
                    <= ((ENEMY_PATHFIND_MAX_RADIUS as i32) * (ENEMY_PATHFIND_MAX_RADIUS as i32))
                {
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
            self.enemy_pick_up_ground_item(i);
            occupied.insert((next.x, next.y), i);
        }
    }

    pub(crate) fn player_attack_power(&self) -> i32 {
        creature_meta("player").attack
            + self.equipped_attack_bonus()
            + (self.level.saturating_sub(1) as i32)
    }

    pub(crate) fn player_agility(&self) -> i32 {
        let base = creature_meta("player").agility + ((self.level.saturating_sub(1) as i32) / 3);
        if self.player_status.slowed_turns > 0 {
            (base - 3).max(1)
        } else {
            base
        }
    }

    fn enemy_agility(&self, idx: usize) -> i32 {
        let base = creature_meta(&self.enemies[idx].creature_id).agility;
        if self.enemies[idx].status.slowed_turns > 0 {
            (base - 3).max(1)
        } else {
            base
        }
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
            "prayer_remnant" => floor >= 1,
            "stone_warden" => floor >= 3,
            "grave_frame" => floor >= 3,
            "incense_shell" => floor >= 3,
            "engorged_husk" => floor >= 3,
            "tomb_warden" => floor >= 4,
            "scavenger_knight" => floor >= 4,
            "dominion_vessel" => floor >= 4,
            "night_choir" => floor >= 4,
            "cathedral_frame" => floor >= 4,
            "coffin_bearer" => floor >= 5,
            "archive_scribe" => floor >= 5,
            "seal_hound" => floor >= 5,
            "buried_ember" => floor >= 5,
            "carrier_frame" => floor >= 7,
            "relay_surgeon" => floor >= 7,
            "specimen_guard" => floor >= 7,
            "flayed_specimen" => floor >= 9,
            _ => true,
        }
    }

    pub(crate) fn current_biome_name(&mut self) -> &'static str {
        self.world.biome_name_at(self.player.x, self.player.y)
    }

    pub(crate) fn biome_id_at(&mut self, x: i32, y: i32) -> u8 {
        self.world.biome_id_at(x, y).0
    }

    fn equipped_attack_bonus(&self) -> i32 {
        let weapon = self
            .equipped_sword
            .as_ref()
            .map(|t| Self::item_attack_bonus(t.kind))
            .unwrap_or(0);
        let ritual = self
            .equipped_sword
            .as_ref()
            .map(|it| it.weapon_bonus)
            .unwrap_or(0)
            .clamp(0, FORGE_SCROLL_ATK_BONUS_MAX);
        let accessory = match self.equipped_accessory.as_ref().map(|t| t.kind) {
            Some(Item::LuckyCharm) => 1,
            _ => 0,
        };
        weapon + ritual + accessory
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
        let facing = Facing::from_delta(to.x - from.x, to.y - from.y).unwrap_or(Facing::E);
        let style = if from.x == self.player.x && from.y == self.player.y {
            "player"
        } else {
            "enemy"
        };
        let color_override = if from.x == self.player.x && from.y == self.player.y {
            self.equipped_sword
                .as_ref()
                .map(|it| item_meta(it.kind).color)
                .or_else(|| Some(creature_meta("player").color))
        } else {
            self.enemies
                .iter()
                .find(|e| e.pos.x == from.x && e.pos.y == from.y)
                .map(|e| {
                    e.equipped_weapon
                        .map(|w| item_meta(w).color)
                        .unwrap_or_else(|| creature_meta(&e.creature_id).color)
                })
        };
        self.push_visual_effect(
            "attack_normal",
            to,
            facing,
            style,
            delay_frames,
            color_override,
        );
    }

    fn push_visual_effect(
        &mut self,
        effect_id: &str,
        origin: Pos,
        facing: Facing,
        style_key: &str,
        delay_frames: u8,
        color_override: Option<Color>,
    ) {
        self.visual_effects.push(ActiveVisualEffect {
            effect_id: effect_id.to_string(),
            origin,
            facing,
            style_key: style_key.to_string(),
            color_override,
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
            self.push_visual_effect("flame_breath", pos, facing, style_key, delay, None);
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
            let effect_color = fx.color_override.unwrap_or(style.color);
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
                        color: effect_color,
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
        let mut next_visual: Vec<ActiveVisualEffect> =
            Vec::with_capacity(self.visual_effects.len());
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
            if let Some(facing) = Facing::from_delta(
                hit.attacker_pos.x - self.player.x,
                hit.attacker_pos.y - self.player.y,
            ) {
                self.facing = facing;
            }
            if self.roll_percent(evade_chance_percent(
                hit.attacker_agility,
                self.player_agility(),
            )) {
                self.push_log_trf("game.you_evaded", &[("enemy", hit.enemy_name)]);
                continue;
            }
            if !self.invincible {
                self.player_hp -= hit.damage;
                self.stat_damage_taken = self.stat_damage_taken.saturating_add(hit.damage as u32);
                if self.player_hp <= 0 && self.death_cause.is_none() {
                    let enemy_name = resolve_log_arg_value(&hit.enemy_name);
                    self.death_cause = Some(trf(
                        "death.cause.enemy",
                        &[("enemy", enemy_name)],
                    ));
                }
                self.push_log_trf(
                    "game.enemy_hit_you",
                    &[
                        ("enemy", hit.enemy_name.clone()),
                        ("damage", hit.damage.to_string()),
                    ],
                );
                self.apply_player_statuses(&hit.enemy_name, hit.burning_turns, hit.slowed_turns);
                any_hit_applied = true;
            }
        }
        self.pending_enemy_hits = next_hits;
        if any_hit_applied && self.player_hp <= 0 {
            self.push_log_tr("game.you_slain");
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
        let mut ground_copper: Vec<GroundCopperState> = self
            .ground_copper
            .iter()
            .map(|(&(x, y), &disks)| GroundCopperState { x, y, disks })
            .collect();
        ground_copper.sort_by_key(|g| (g.x, g.y));

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
            player_status: self.player_status,
            player_mp: self.player_mp,
            player_max_mp: self.player_max_mp,
            player_hunger: self.player_hunger,
            player_max_hunger: self.player_max_hunger,
            player_copper_disks: self.player_copper_disks,
            legacy_weapon_ritual_bonus: 0,
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
                    status: e.status,
                    carried_items: e.carried_items.clone(),
                    equipped_weapon: e.equipped_weapon,
                    facing: e.facing,
                    flee_from_player: e.flee_from_player,
                })
                .collect(),
            ground_items,
            ground_copper,
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
            stone_tablets: self
                .stone_tablets
                .iter()
                .map(|(&(x, y), &kind)| StoneTabletState { x, y, kind })
                .collect(),
            structures: self
                .structures
                .iter()
                .map(|(&(x, y), &kind)| StructureState { x, y, kind })
                .collect(),
            substory_facility: self.substory_facility.map(|state| SubstoryFacilitySnapshotState {
                center_x: state.center.x,
                center_y: state.center.y,
                kind: state.kind,
                guardian_id: state.guardian_id,
                cleared: state.cleared,
            }),
            substory_facility_attempted: self.substory_facility_attempted,
            ancient_attuned_sites: self
                .ancient_attuned_sites
                .iter()
                .map(|&(x, y)| PosState { x, y })
                .collect(),
            ancient_awakened_sites: self
                .ancient_awakened_sites
                .iter()
                .map(|&(x, y)| PosState { x, y })
                .collect(),
            ancient_charge: self.ancient_charge,
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
            lang_code: current_lang().to_string(),
            logs: self.logs.clone(),
        }
    }

    pub(crate) fn from_snapshot(snapshot: GameSnapshot) -> Result<Self, String> {
        let mut snapshot = snapshot;
        let restored_floor = snapshot.floor.max(1);
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
        let mut ground_copper = HashMap::new();
        for g in snapshot.ground_copper {
            ground_copper.insert((g.x, g.y), g.disks);
        }
        let mut blood_stains = HashSet::new();
        for b in snapshot.blood_stains {
            blood_stains.insert((b.x, b.y));
        }
        let mut torches = HashSet::new();
        for t in snapshot.torches {
            torches.insert((t.x, t.y));
        }
        let mut stone_tablets = HashMap::new();
        for t in snapshot.stone_tablets {
            stone_tablets.insert((t.x, t.y), t.kind);
        }
        let mut structures = HashMap::new();
        for s in snapshot.structures {
            structures.insert((s.x, s.y), s.kind);
        }
        let substory_facility = snapshot.substory_facility.map(|state| SubstoryFacilityState {
            center: Pos {
                x: state.center_x,
                y: state.center_y,
            },
            kind: state.kind,
            guardian_id: state.guardian_id,
            cleared: state.cleared,
        });
        let mut ancient_attuned_sites = HashSet::new();
        for p in snapshot.ancient_attuned_sites {
            ancient_attuned_sites.insert((p.x, p.y));
        }
        let mut ancient_awakened_sites = HashSet::new();
        for p in snapshot.ancient_awakened_sites {
            ancient_awakened_sites.insert((p.x, p.y));
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
                    status: e.status,
                    carried_items: e.carried_items,
                    equipped_weapon: e.equipped_weapon,
                    facing: e.facing,
                    flee_from_player: e.flee_from_player,
                }
            })
            .collect();

        let _ = set_lang(&snapshot.lang_code);

        let mut next_entity_id = snapshot.next_entity_id.max(next_auto_enemy_id).max(1);
        let mut inventory = std::mem::take(&mut snapshot.inventory);
        for item in &mut inventory {
            item.weapon_bonus = item.weapon_bonus.clamp(0, FORGE_SCROLL_ATK_BONUS_MAX);
            if item.uid == 0 {
                item.uid = next_entity_id;
                next_entity_id = next_entity_id.saturating_add(1);
            } else if item.uid >= next_entity_id {
                next_entity_id = item.uid.saturating_add(1);
            }
        }
        let mut equipped_sword = std::mem::take(&mut snapshot.equipped_sword);
        let mut equipped_shield = std::mem::take(&mut snapshot.equipped_shield);
        let mut equipped_accessory = std::mem::take(&mut snapshot.equipped_accessory);
        for equipped in [
            &mut equipped_sword,
            &mut equipped_shield,
            &mut equipped_accessory,
        ] {
            let Some(eq) = equipped.as_mut() else {
                continue;
            };
            eq.weapon_bonus = eq.weapon_bonus.clamp(0, FORGE_SCROLL_ATK_BONUS_MAX);
            if eq.uid == 0 {
                if let Some(found) = inventory
                    .iter()
                    .find(|it| it.kind == eq.kind && it.custom_name == eq.custom_name)
                {
                    eq.uid = found.uid;
                } else {
                    eq.uid = next_entity_id;
                    next_entity_id = next_entity_id.saturating_add(1);
                }
            } else if eq.uid >= next_entity_id {
                next_entity_id = eq.uid.saturating_add(1);
            }
        }
        let legacy_bonus = snapshot
            .legacy_weapon_ritual_bonus
            .clamp(0, FORGE_SCROLL_ATK_BONUS_MAX);
        if legacy_bonus > 0 {
            if let Some(eq) = equipped_sword.as_mut() {
                eq.weapon_bonus = eq.weapon_bonus.max(legacy_bonus);
                let equipped_copy = eq.clone();
                if let Some(inv_item) = inventory
                    .iter_mut()
                    .find(|it| it.same_identity(&equipped_copy))
                {
                    inv_item.weapon_bonus = inv_item.weapon_bonus.max(eq.weapon_bonus);
                }
            }
        }

        Ok(Self {
            world: World {
                seed: snapshot.seed,
                biome_palette: crate::world_cfg::biomes_for_floor(restored_floor),
                map_pattern: World::pattern_for_floor(restored_floor),
                terrain_theme: World::theme_for_floor(restored_floor),
                stairs_mode: World::stairs_mode_for_floor(restored_floor),
                special_facility_mode: World::special_facility_mode_for_floor(restored_floor),
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
            player_status: snapshot.player_status,
            player_mp: snapshot.player_mp.clamp(0, snapshot.player_max_mp.max(1)),
            player_max_mp: snapshot.player_max_mp.max(1),
            player_hunger: snapshot
                .player_hunger
                .clamp(0, snapshot.player_max_hunger.max(1)),
            player_max_hunger: snapshot.player_max_hunger.max(1),
            player_copper_disks: snapshot.player_copper_disks,
            inventory,
            equipped_sword,
            equipped_shield,
            equipped_accessory,
            enemies,
            ground_items,
            ground_copper,
            blood_stains,
            torches,
            stone_tablets,
            structures,
            substory_facility,
            substory_facility_attempted: snapshot.substory_facility_attempted,
            ancient_attuned_sites,
            ancient_awakened_sites,
            ancient_charge: snapshot.ancient_charge.min(9),
            attack_effects: Vec::new(),
            visual_effects: Vec::new(),
            pending_enemy_hits: Vec::new(),
            harvest_state: snapshot.harvest_state.map(|h| HarvestState {
                target: (h.x, h.y),
                hits: h.hits,
            }),
            rng_state: snapshot.rng_state,
            next_entity_id,
            turn: snapshot.turn,
            floor: restored_floor,
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
            pending_popup: None,
            pending_vending: false,
            suppress_auto_pickup_once: false,
            invincible: false,
            death_cause: None,
        })
    }
}

fn calc_damage(attack: i32, defense: i32) -> i32 {
    (attack - defense).max(1)
}

fn evade_chance_percent(attacker_agility: i32, defender_agility: i32) -> u8 {
    // Base on agility difference, but reduce chance when both are fast.
    // This makes high-AGI vs high-AGI exchanges harder to evade for both sides.
    let diff_term = (defender_agility - attacker_agility) * 4;
    let speed_pressure = (attacker_agility + defender_agility) / 3;
    let chance = 14 + diff_term - speed_pressure;
    chance.clamp(1, 60) as u8
}

fn apply_status_bundle(status: &mut StatusState, burning_turns: u8, slowed_turns: u8) {
    status.burning_turns = status.burning_turns.max(burning_turns);
    status.slowed_turns = status.slowed_turns.max(slowed_turns);
}

fn destructible_info(tile: Tile) -> Option<(u8, Option<Item>, u8, Tile, &'static str)> {
    let meta = tile_meta(tile);
    let hits = meta.harvest_hits?;
    let replace = meta.harvest_replace?;
    let label = meta
        .harvest_label
        .as_deref()
        .unwrap_or(meta.legend.as_str());
    Some((
        hits,
        meta.harvest_drop,
        meta.harvest_drop_chance,
        replace,
        label,
    ))
}
