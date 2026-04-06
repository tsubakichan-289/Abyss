mod defs;
mod game;
mod noise;
mod save;
mod text;
mod world_cfg;

use std::collections::{HashMap, HashSet};
use std::env;
use std::io;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use defs::{RecipeDef, creature_meta, defs, item_meta, tile_meta};
use game::{Action, Game, StoneTabletKind, StructureKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use rodio::source::{SineWave, white};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use serde::{Deserialize, Serialize};
use text::{available_languages, current_lang, set_lang, tr, trf};

const CHUNK_SIZE: usize = 16;
const CHUNK_AREA: usize = CHUNK_SIZE * CHUNK_SIZE;
const VISION_RADIUS: i32 = 7;
const ABYSS_THRESHOLD: f64 = 0.22;
const ROCK_THRESHOLD: f64 = 0.63;
const WALL_THRESHOLD: f64 = 0.80;
const ESC_HOLD_STEPS: u8 = 8;
const POTION_HEAL: i32 = 6;
const TURN_REGEN_INTERVAL: u64 = 8;
const MAX_INVENTORY: usize = 20;
const SAVE_FILE_BASENAME: &str = "savegame";
const SAVE_SLOT_COUNT: usize = 3;
const TITLE_LOGO: &str = include_str!("../logo_width_120.txt");

#[derive(Clone, Copy)]
enum SfxCue {
    Step,
    AttackSwing,
    AttackHit,
    EnemyDown,
    EnemyAttack,
    Pickup,
    Use,
    Throw,
    Drop,
    Stairs,
    Craft,
}

struct SoundPlayer {
    _stream: OutputStream,
    handle: OutputStreamHandle,
}

impl SoundPlayer {
    fn new() -> Option<Self> {
        let (stream, handle) = OutputStream::try_default().ok()?;
        Some(Self {
            _stream: stream,
            handle,
        })
    }

    fn play(&self, cue: SfxCue) {
        let Ok(sink) = Sink::try_new(&self.handle) else {
            return;
        };
        match cue {
            SfxCue::Step => {
                sink.append(
                    SineWave::new(160.0)
                        .take_duration(Duration::from_millis(24))
                        .amplify(0.06),
                );
            }
            SfxCue::AttackSwing => {
                self.append_slash_noise(&sink, 380.0, 0.10);
            }
            SfxCue::AttackHit => {
                self.append_slash_noise(&sink, 520.0, 0.14);
            }
            SfxCue::EnemyDown => {
                sink.append(
                    SineWave::new(620.0)
                        .take_duration(Duration::from_millis(42))
                        .amplify(0.12),
                );
                sink.append(
                    SineWave::new(320.0)
                        .take_duration(Duration::from_millis(58))
                        .amplify(0.10),
                );
            }
            SfxCue::EnemyAttack => {
                self.append_slash_noise(&sink, 300.0, 0.16);
            }
            SfxCue::Pickup => {
                sink.append(
                    SineWave::new(740.0)
                        .take_duration(Duration::from_millis(30))
                        .amplify(0.08),
                );
            }
            SfxCue::Use => {
                sink.append(
                    SineWave::new(460.0)
                        .take_duration(Duration::from_millis(40))
                        .amplify(0.08),
                );
            }
            SfxCue::Throw => {
                sink.append(
                    SineWave::new(520.0)
                        .take_duration(Duration::from_millis(35))
                        .amplify(0.09),
                );
            }
            SfxCue::Drop => {
                sink.append(
                    SineWave::new(220.0)
                        .take_duration(Duration::from_millis(40))
                        .amplify(0.08),
                );
            }
            SfxCue::Stairs => {
                sink.append(
                    SineWave::new(480.0)
                        .take_duration(Duration::from_millis(40))
                        .amplify(0.10),
                );
                sink.append(
                    SineWave::new(640.0)
                        .take_duration(Duration::from_millis(50))
                        .amplify(0.10),
                );
            }
            SfxCue::Craft => {
                sink.append(
                    SineWave::new(520.0)
                        .take_duration(Duration::from_millis(35))
                        .amplify(0.10),
                );
                sink.append(
                    SineWave::new(780.0)
                        .take_duration(Duration::from_millis(45))
                        .amplify(0.08),
                );
            }
        }
        sink.detach();
    }

    fn append_slash_noise(&self, sink: &Sink, base_hz: f32, amp: f32) {
        let noise = white(rodio::cpal::SampleRate(44100))
            .take_duration(Duration::from_millis(45))
            .amplify(amp * 0.62);
        let body = SineWave::new(base_hz)
            .take_duration(Duration::from_millis(40))
            .amplify(amp * 0.35);
        let edge = SineWave::new(base_hz * 2.13)
            .take_duration(Duration::from_millis(32))
            .amplify(amp * 0.22);
        sink.append(noise.mix(body).mix(edge));
    }
}

#[derive(Clone, Copy)]
struct TurnSnapshot {
    player_x: i32,
    player_y: i32,
    damage_dealt: u32,
    damage_taken: u32,
    enemies_defeated: u32,
    items_picked: u32,
}

impl TurnSnapshot {
    fn capture(game: &Game) -> Self {
        Self {
            player_x: game.player.x,
            player_y: game.player.y,
            damage_dealt: game.stat_damage_dealt,
            damage_taken: game.stat_damage_taken,
            enemies_defeated: game.stat_enemies_defeated,
            items_picked: game.stat_items_picked,
        }
    }
}

fn play_turn_sfx(sfx: &Option<SoundPlayer>, before: TurnSnapshot, after: &Game, fallback: Option<SfxCue>) {
    let Some(sfx) = sfx.as_ref() else {
        return;
    };
    if after.stat_damage_taken > before.damage_taken {
        sfx.play(SfxCue::EnemyAttack);
        return;
    }
    if after.stat_enemies_defeated > before.enemies_defeated {
        sfx.play(SfxCue::EnemyDown);
        return;
    }
    if after.stat_damage_dealt > before.damage_dealt {
        sfx.play(SfxCue::AttackHit);
        return;
    }
    if after.stat_items_picked > before.items_picked {
        sfx.play(SfxCue::Pickup);
        return;
    }
    let moved = after.player.x != before.player_x || after.player.y != before.player_y;
    if moved {
        if let Some(SfxCue::Step) = fallback {
            sfx.play(SfxCue::Step);
            return;
        }
    }
    if let Some(cue) = fallback {
        sfx.play(cue);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Tile {
    Abyss,
    DeepWater,
    ShallowWater,
    Sand,
    Grass,
    Forest,
    Mountain,
    Rock,
    Wall,
    StairsDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Item {
    Potion,
    Herb,
    Elixir,
    Food,
    Bread,
    Torch,
    FlameScroll,
    EmberScroll,
    BlinkScroll,
    BindScroll,
    RepulseScroll,
    NovaScroll,
    PulseBomb,
    ForgeScroll,
    GladiusNadir,
    FerrumOccasus,
    VirgaOriens,
    VirgaMeridies,
    VirgaZenith,
    Wood,
    Stone,
    IronIngot,
    Hide,
    StringFiber,
    StoneAxe,
    IronSword,
    IronPickaxe,
    WoodenShield,
    LuckyCharm,
    QuartzMemoryKnowledge,
    QuartzMemoryLife,
    QuartzMemoryDimension,
    QuartzMemoryInterface,
    QuartzMemoryExtraction,
    QuartzMemoryArchive,
    QuartzMemoryCathedral,
    QuartzMemoryHalo,
    QuartzMemoryLung,
    QuartzMemoryOssuary,
    QuartzMemoryChoir,
    QuartzMemoryWitness,
}

impl Item {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Potion => "potion",
            Self::Herb => "herb",
            Self::Elixir => "elixir",
            Self::Food => "jerky",
            Self::Bread => "bread",
            Self::Torch => "torch",
            Self::FlameScroll => "flame_scroll",
            Self::EmberScroll => "ember_scroll",
            Self::BlinkScroll => "blink_scroll",
            Self::BindScroll => "bind_scroll",
            Self::RepulseScroll => "repulse_scroll",
            Self::NovaScroll => "nova_scroll",
            Self::PulseBomb => "pulse_bomb",
            Self::ForgeScroll => "forge_scroll",
            Self::GladiusNadir => "gladius_nadir",
            Self::FerrumOccasus => "ferrum_occasus",
            Self::VirgaOriens => "virga_oriens",
            Self::VirgaMeridies => "virga_meridies",
            Self::VirgaZenith => "virga_zenith",
            Self::Wood => "wood",
            Self::Stone => "stone",
            Self::IronIngot => "iron_ingot",
            Self::Hide => "hide",
            Self::StringFiber => "string",
            Self::StoneAxe => "stone_axe",
            Self::IronSword => "iron_sword",
            Self::IronPickaxe => "iron_pickaxe",
            Self::WoodenShield => "wooden_shield",
            Self::LuckyCharm => "lucky_charm",
            Self::QuartzMemoryKnowledge => "quartz_memory_knowledge",
            Self::QuartzMemoryLife => "quartz_memory_life",
            Self::QuartzMemoryDimension => "quartz_memory_dimension",
            Self::QuartzMemoryInterface => "quartz_memory_interface",
            Self::QuartzMemoryExtraction => "quartz_memory_extraction",
            Self::QuartzMemoryArchive => "quartz_memory_archive",
            Self::QuartzMemoryCathedral => "quartz_memory_cathedral",
            Self::QuartzMemoryHalo => "quartz_memory_halo",
            Self::QuartzMemoryLung => "quartz_memory_lung",
            Self::QuartzMemoryOssuary => "quartz_memory_ossuary",
            Self::QuartzMemoryChoir => "quartz_memory_choir",
            Self::QuartzMemoryWitness => "quartz_memory_witness",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "potion" => Some(Self::Potion),
            "herb" => Some(Self::Herb),
            "elixir" => Some(Self::Elixir),
            "jerky" | "food" => Some(Self::Food),
            "bread" => Some(Self::Bread),
            "torch" => Some(Self::Torch),
            "flame_scroll" => Some(Self::FlameScroll),
            "ember_scroll" => Some(Self::EmberScroll),
            "blink_scroll" => Some(Self::BlinkScroll),
            "bind_scroll" => Some(Self::BindScroll),
            "repulse_scroll" => Some(Self::RepulseScroll),
            "nova_scroll" => Some(Self::NovaScroll),
            "pulse_bomb" => Some(Self::PulseBomb),
            "forge_scroll" => Some(Self::ForgeScroll),
            "gladius_nadir" => Some(Self::GladiusNadir),
            "ferrum_occasus" => Some(Self::FerrumOccasus),
            "virga_oriens" => Some(Self::VirgaOriens),
            "virga_meridies" => Some(Self::VirgaMeridies),
            "virga_zenith" => Some(Self::VirgaZenith),
            "wood" => Some(Self::Wood),
            "stone" => Some(Self::Stone),
            "iron_ingot" => Some(Self::IronIngot),
            "hide" => Some(Self::Hide),
            "string" => Some(Self::StringFiber),
            "stone_axe" => Some(Self::StoneAxe),
            "iron_sword" => Some(Self::IronSword),
            "iron_pickaxe" => Some(Self::IronPickaxe),
            "wooden_shield" => Some(Self::WoodenShield),
            "lucky_charm" => Some(Self::LuckyCharm),
            "quartz_memory_knowledge" => Some(Self::QuartzMemoryKnowledge),
            "quartz_memory_life" => Some(Self::QuartzMemoryLife),
            "quartz_memory_dimension" => Some(Self::QuartzMemoryDimension),
            "quartz_memory_interface" => Some(Self::QuartzMemoryInterface),
            "quartz_memory_extraction" => Some(Self::QuartzMemoryExtraction),
            "quartz_memory_archive" => Some(Self::QuartzMemoryArchive),
            "quartz_memory_cathedral" => Some(Self::QuartzMemoryCathedral),
            "quartz_memory_halo" => Some(Self::QuartzMemoryHalo),
            "quartz_memory_lung" => Some(Self::QuartzMemoryLung),
            "quartz_memory_ossuary" => Some(Self::QuartzMemoryOssuary),
            "quartz_memory_choir" => Some(Self::QuartzMemoryChoir),
            "quartz_memory_witness" => Some(Self::QuartzMemoryWitness),
            _ => None,
        }
    }

    fn glyph(self) -> char {
        item_meta(self).glyph
    }

    fn color(self) -> Color {
        item_meta(self).color
    }
}

fn tr_or_fallback(key: String, fallback: &str) -> String {
    let val = tr(&key);
    if val == key {
        fallback.to_string()
    } else {
        val.to_string()
    }
}

pub(crate) fn localized_item_name(item: Item) -> String {
    tr_or_fallback(format!("item.name.{}", item.key()), &item_meta(item).name)
}

pub(crate) fn log_arg_item_ref(item: Item) -> String {
    format!("\u{1f}item:{}", item.key())
}

pub(crate) fn log_arg_inventory_item_ref(item: &InventoryItem) -> String {
    match &item.custom_name {
        Some(name) if !name.is_empty() => name.clone(),
        _ => log_arg_item_ref(item.kind),
    }
}

pub(crate) fn log_arg_creature_ref(id: &str) -> String {
    format!("\u{1f}creature:{id}")
}

pub(crate) fn log_arg_text_ref(key: &str) -> String {
    format!("\u{1f}text:{key}")
}

fn localized_item_status(item: Item) -> String {
    tr_or_fallback(
        format!("item.status.{}", item.key()),
        &item_meta(item).status,
    )
}

fn localized_item_description(item: Item) -> String {
    if item == Item::ForgeScroll {
        return localized_forge_scroll_description();
    }
    tr_or_fallback(
        format!("item.description.{}", item.key()),
        &item_meta(item).description,
    )
}

fn localized_forge_scroll_description() -> String {
    let pattern = crate::defs::forge_scroll_pattern_lines().join("\n");
    let key = "item.description.forge_scroll.template";
    let tmpl = tr(key);
    if tmpl != key {
        return tmpl.replace("{pattern}", &pattern);
    }
    format!(
        "Ritual scroll.\n{}\nUse on blood stain (@) to strengthen equipped weapon ATK.",
        pattern
    )
}

pub(crate) fn localized_creature_name(id: &str) -> String {
    tr_or_fallback(format!("creature.name.{id}"), &creature_meta(id).name)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct InventoryItem {
    #[serde(default = "default_inventory_uid")]
    uid: u64,
    kind: Item,
    custom_name: Option<String>,
    #[serde(default = "default_inventory_weapon_bonus")]
    weapon_bonus: i32,
    #[serde(default = "default_inventory_qty")]
    qty: u16,
}

fn default_inventory_uid() -> u64 {
    0
}

fn default_inventory_qty() -> u16 {
    1
}

fn default_inventory_weapon_bonus() -> i32 {
    0
}

impl InventoryItem {
    pub(crate) fn same_identity(&self, other: &InventoryItem) -> bool {
        if self.uid != 0 && other.uid != 0 {
            self.uid == other.uid
        } else {
            self.kind == other.kind && self.custom_name == other.custom_name
        }
    }

    fn display_name(&self) -> String {
        match &self.custom_name {
            Some(name) if !name.is_empty() => name.clone(),
            _ => localized_item_name(self.kind),
        }
    }

    fn display_name_with_qty(&self) -> String {
        if self.qty > 1 {
            format!("{} x{}", self.display_name(), self.qty)
        } else {
            self.display_name()
        }
    }
}

impl Tile {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Abyss => "abyss",
            Self::DeepWater => "deep_water",
            Self::ShallowWater => "shallow_water",
            Self::Sand => "sand",
            Self::Grass => "grass",
            Self::Forest => "forest",
            Self::Mountain => "mountain",
            Self::Rock => "rock",
            Self::Wall => "wall",
            Self::StairsDown => "stairs_down",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "abyss" => Some(Self::Abyss),
            "deep_water" => Some(Self::DeepWater),
            "shallow_water" => Some(Self::ShallowWater),
            "sand" => Some(Self::Sand),
            "grass" => Some(Self::Grass),
            "forest" => Some(Self::Forest),
            "mountain" => Some(Self::Mountain),
            "rock" => Some(Self::Rock),
            "wall" => Some(Self::Wall),
            "stairs_down" => Some(Self::StairsDown),
            _ => None,
        }
    }

    fn from_height(h: f64) -> Self {
        if h < 0.34 {
            Self::DeepWater
        } else if h < 0.45 {
            Self::ShallowWater
        } else if h < 0.52 {
            Self::Sand
        } else if h < 0.60 {
            Self::Grass
        } else if h < 0.72 {
            Self::Forest
        } else {
            Self::Mountain
        }
    }

    fn glyph(self) -> char {
        tile_meta(self).glyph
    }

    fn color(self) -> Color {
        tile_meta(self).color
    }

    fn walkable(self) -> bool {
        tile_meta(self).walkable
    }
}

fn shadow_color(tile: Tile) -> Color {
    tile_meta(tile).shadow_color
}

fn biome_brightness_shift(biome_id: u8) -> f32 {
    match biome_id {
        // Brighter biomes.
        2 | 3 | 5 | 6 | 7 => 0.08,
        // Darker/deep biomes.
        11 | 12 | 13 | 14 | 15 => -0.11,
        // Neutral biomes.
        _ => 0.0,
    }
}

fn apply_brightness_shift(color: Color, shift: f32) -> Color {
    fn xterm_index_to_rgb(idx: u8) -> (u8, u8, u8) {
        const ANSI16: [(u8, u8, u8); 16] = [
            (0, 0, 0),
            (205, 0, 0),
            (0, 205, 0),
            (205, 205, 0),
            (0, 0, 238),
            (205, 0, 205),
            (0, 205, 205),
            (229, 229, 229),
            (127, 127, 127),
            (255, 0, 0),
            (0, 255, 0),
            (255, 255, 0),
            (92, 92, 255),
            (255, 0, 255),
            (0, 255, 255),
            (255, 255, 255),
        ];
        if idx < 16 {
            return ANSI16[idx as usize];
        }
        if idx >= 232 {
            let v = 8u8.saturating_add((idx - 232).saturating_mul(10));
            return (v, v, v);
        }
        let n = idx - 16;
        let r = n / 36;
        let g = (n % 36) / 6;
        let b = n % 6;
        let step = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
        (step(r), step(g), step(b))
    }
    fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
        let rf = r as f32 / 255.0;
        let gf = g as f32 / 255.0;
        let bf = b as f32 / 255.0;
        let max = rf.max(gf.max(bf));
        let min = rf.min(gf.min(bf));
        let l = (max + min) * 0.5;
        if (max - min).abs() < f32::EPSILON {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
        let h = if (max - rf).abs() < f32::EPSILON {
            60.0 * (((gf - bf) / d) % 6.0)
        } else if (max - gf).abs() < f32::EPSILON {
            60.0 * (((bf - rf) / d) + 2.0)
        } else {
            60.0 * (((rf - gf) / d) + 4.0)
        };
        (if h < 0.0 { h + 360.0 } else { h }, s.clamp(0.0, 1.0), l)
    }
    fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
        if s <= 1e-6 {
            let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
            return (v, v, v);
        }
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let hp = (h / 60.0) % 6.0;
        let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
        let (r1, g1, b1) = if hp < 1.0 {
            (c, x, 0.0)
        } else if hp < 2.0 {
            (x, c, 0.0)
        } else if hp < 3.0 {
            (0.0, c, x)
        } else if hp < 4.0 {
            (0.0, x, c)
        } else if hp < 5.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        let m = l - c * 0.5;
        let to_u8 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        (to_u8(r1), to_u8(g1), to_u8(b1))
    }
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(v) => xterm_index_to_rgb(v),
        _ => return color,
    };
    let (h, s, l) = rgb_to_hsl(r, g, b);
    let l2 = (l + shift).clamp(0.0, 1.0);
    let (nr, ng, nb) = hsl_to_rgb(h, s, l2);
    Color::Rgb(nr, ng, nb)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Facing {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Facing {
    fn from_delta(dx: i32, dy: i32) -> Option<Self> {
        match (dx.signum(), dy.signum()) {
            (0, -1) => Some(Self::N),
            (1, -1) => Some(Self::NE),
            (1, 0) => Some(Self::E),
            (1, 1) => Some(Self::SE),
            (0, 1) => Some(Self::S),
            (-1, 1) => Some(Self::SW),
            (-1, 0) => Some(Self::W),
            (-1, -1) => Some(Self::NW),
            _ => None,
        }
    }

    fn delta(self) -> (i32, i32) {
        match self {
            Self::N => (0, -1),
            Self::NE => (1, -1),
            Self::E => (1, 0),
            Self::SE => (1, 1),
            Self::S => (0, 1),
            Self::SW => (-1, 1),
            Self::W => (-1, 0),
            Self::NW => (-1, -1),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::N => tr("dir.n"),
            Self::NE => tr("dir.ne"),
            Self::E => tr("dir.e"),
            Self::SE => tr("dir.se"),
            Self::S => tr("dir.s"),
            Self::SW => tr("dir.sw"),
            Self::W => tr("dir.w"),
            Self::NW => tr("dir.nw"),
        }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ItemMenuAction {
    Rename,
    Drop,
    Throw,
    Use,
}

impl ItemMenuAction {
    fn label(self) -> &'static str {
        match self {
            Self::Rename => tr("item_menu.rename"),
            Self::Drop => tr("item_menu.drop"),
            Self::Throw => tr("item_menu.throw"),
            Self::Use => tr("item_menu.use"),
        }
    }
}

const ITEM_MENU_ACTIONS: [ItemMenuAction; 4] = [
    ItemMenuAction::Rename,
    ItemMenuAction::Drop,
    ItemMenuAction::Throw,
    ItemMenuAction::Use,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroundItemMenuAction {
    Pick,
    Swap,
    Use,
    Throw,
    Rename,
}

impl GroundItemMenuAction {
    fn label(self) -> &'static str {
        match self {
            Self::Pick => tr("item_menu.pick"),
            Self::Swap => tr("item_menu.swap"),
            Self::Use => tr("item_menu.use"),
            Self::Throw => tr("item_menu.throw"),
            Self::Rename => tr("item_menu.rename"),
        }
    }
}

const GROUND_ITEM_MENU_ACTIONS: [GroundItemMenuAction; 5] = [
    GroundItemMenuAction::Rename,
    GroundItemMenuAction::Throw,
    GroundItemMenuAction::Use,
    GroundItemMenuAction::Pick,
    GroundItemMenuAction::Swap,
];

#[derive(Clone, Debug)]
enum UiMode {
    Title {
        selected: usize,
    },
    TitleSlotSelect {
        selected: usize,
        for_load: bool,
    },
    TitleDeleteConfirm {
        selected: usize,
    },
    TitleTextTest {
        nonce: u64,
        last_tick: Instant,
    },
    Normal,
    DebugConsole {
        input: String,
        suggestion_idx: usize,
    },
    StairsPrompt {
        selected_action: StairsAction,
    },
    MainMenu {
        selected: usize,
    },
    Inventory {
        selected: usize,
        move_selected: bool,
    },
    ItemMenu {
        selected: usize,
        action_idx: usize,
    },
    GroundItemMenu {
        action_idx: usize,
    },
    GroundSwapSelect {
        selected: usize,
    },
    RenameItem {
        selected: usize,
        input: String,
    },
    Settings {
        selected: usize,
        from_title: bool,
    },
    Hints,
    Dialogue {
        title: String,
        text: String,
    },
    Vending {
        cursor: usize,
        inserted_disks: u32,
    },
    Crafting {
        cursor: usize,
        selected_inv: usize,
        focus: CraftFocus,
        grid: [Option<InventoryItem>; 9],
    },
    Dead {
        scroll: usize,
        selected_action: DeadAction,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CraftFocus {
    Grid,
    Inventory,
    CraftButton,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeadAction {
    Restart,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StairsAction {
    Descend,
    Stay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainMenuEntry {
    Items,
    Crafting,
    Settings,
    Hints,
    Feet,
    Title,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TitleMenuEntry {
    Start,
    Settings,
    TextTest,
    Exit,
}

impl TitleMenuEntry {
    fn label(self) -> &'static str {
        match self {
            Self::Start => tr("title_menu.start"),
            Self::Settings => tr("title_menu.settings"),
            Self::TextTest => tr("title_menu.text_test"),
            Self::Exit => tr("title_menu.exit"),
        }
    }
}

const TITLE_MENU_ENTRIES: [TitleMenuEntry; 4] = [
    TitleMenuEntry::Start,
    TitleMenuEntry::Settings,
    TitleMenuEntry::TextTest,
    TitleMenuEntry::Exit,
];

impl MainMenuEntry {
    fn label(self) -> &'static str {
        match self {
            Self::Items => tr("main_menu.items"),
            Self::Crafting => tr("main_menu.crafting"),
            Self::Settings => tr("main_menu.settings"),
            Self::Hints => tr("main_menu.hints"),
            Self::Feet => tr("main_menu.feet"),
            Self::Title => tr("main_menu.title"),
            Self::Exit => tr("main_menu.exit"),
        }
    }
}

const MAIN_MENU_ENTRIES: [MainMenuEntry; 7] = [
    MainMenuEntry::Items,
    MainMenuEntry::Crafting,
    MainMenuEntry::Settings,
    MainMenuEntry::Hints,
    MainMenuEntry::Feet,
    MainMenuEntry::Title,
    MainMenuEntry::Exit,
];

#[derive(Clone, Copy)]
struct VendingProduct {
    item: Item,
    price_as: u32,
}

const VENDING_PRODUCTS: [VendingProduct; 12] = [
    VendingProduct {
        item: Item::Potion,
        price_as: 2,
    },
    VendingProduct {
        item: Item::Herb,
        price_as: 1,
    },
    VendingProduct {
        item: Item::Bread,
        price_as: 3,
    },
    VendingProduct {
        item: Item::Torch,
        price_as: 2,
    },
    VendingProduct {
        item: Item::FlameScroll,
        price_as: 4,
    },
    VendingProduct {
        item: Item::BlinkScroll,
        price_as: 5,
    },
    VendingProduct {
        item: Item::EmberScroll,
        price_as: 5,
    },
    VendingProduct {
        item: Item::BindScroll,
        price_as: 5,
    },
    VendingProduct {
        item: Item::RepulseScroll,
        price_as: 7,
    },
    VendingProduct {
        item: Item::PulseBomb,
        price_as: 6,
    },
    VendingProduct {
        item: Item::Elixir,
        price_as: 8,
    },
    VendingProduct {
        item: Item::IronIngot,
        price_as: 4,
    },
];

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn render_ui(frame: &mut Frame, game: &mut Game, esc_hold_count: u8, ui_mode: &UiMode) {
    if let UiMode::Title { selected } = ui_mode {
        render_title_screen(frame, game, *selected, has_save_file());
        return;
    }
    if let UiMode::TitleSlotSelect { selected, for_load } = ui_mode {
        render_title_slot_screen(frame, game, *selected, *for_load);
        return;
    }
    if let UiMode::TitleDeleteConfirm { selected } = ui_mode {
        render_title_slot_screen(frame, game, *selected, false);
        let area = centered_rect(44, 22, frame.area());
        frame.render_widget(Clear, area);
        let widget = Paragraph::new(build_title_delete_confirm_lines(*selected))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title(tr("title_slots.delete_title")),
            )
            .wrap(ratatui::widgets::Wrap { trim: false });
        frame.render_widget(widget, area);
        return;
    }
    if let UiMode::TitleTextTest { nonce, .. } = ui_mode {
        render_title_text_test(frame, game, *nonce);
        return;
    }
    if let UiMode::Settings {
        from_title: true, ..
    } = ui_mode
    {
        render_title_backdrop(frame, game);
        let area = centered_rect(50, 40, frame.area());
        frame.render_widget(Clear, area);
        if let UiMode::Settings { selected, .. } = ui_mode {
            let widget = Paragraph::new(build_settings_lines(*selected))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.settings")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        return;
    }
    let areas = Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(frame.area());
    let side_areas =
        Layout::vertical([Constraint::Length(16), Constraint::Min(10)]).split(areas[1]);
    let left_areas = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).split(areas[0]);

    let map_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_chrome_color(game)))
        .title(tr("title.map"));
    let map_inner = map_block.inner(left_areas[0]);
    let map_lines = build_map_lines(game, map_inner.width, map_inner.height);
    let map_widget = Paragraph::new(map_lines).block(map_block);
    frame.render_widget(map_widget, left_areas[0]);

    let quick_widget = Paragraph::new(build_quickbar_line(game))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_chrome_color(game)))
                .title(tr("title.quick")),
        )
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(quick_widget, left_areas[1]);

    let status_widget = Paragraph::new(build_status_lines(game, esc_hold_count))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_chrome_color(game)))
                .title(tr("title.status")),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(status_widget, side_areas[0]);

    let log_height = side_areas[1].height.saturating_sub(2) as usize;
    let log_widget = Paragraph::new(build_log_lines(game, log_height))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_chrome_color(game)))
                .title(tr("title.log")),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(log_widget, side_areas[1]);

    match ui_mode {
        UiMode::Title { .. } => {}
        UiMode::TitleSlotSelect { .. } => {}
        UiMode::TitleDeleteConfirm { .. } => {}
        UiMode::TitleTextTest { .. } => {}
        UiMode::Normal => {}
        UiMode::DebugConsole {
            input,
            suggestion_idx,
        } => {
            let area = centered_rect(70, 28, frame.area());
            frame.render_widget(Clear, area);
            let inner_height = area.height.saturating_sub(2) as usize;
            let widget = Paragraph::new(build_debug_console_lines(
                input,
                *suggestion_idx,
                inner_height,
            ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.debug")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::StairsPrompt { selected_action } => {
            let area = centered_rect(42, 24, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_stairs_prompt_lines(*selected_action))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.stairs")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::MainMenu { selected } => {
            let area = centered_rect(40, 45, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_main_menu_lines(*selected))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.menu")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::Inventory {
            selected,
            move_selected,
        } => {
            render_inventory_modal(frame, game, *selected, *move_selected);
        }
        UiMode::Settings {
            selected,
            from_title: _,
        } => {
            let area = centered_rect(50, 40, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_settings_lines(*selected))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.settings")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::Hints => {
            let area = centered_rect(55, 55, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_hints_lines())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.hints")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::Dialogue { title, text } => {
            let area = centered_rect(52, 38, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_dialogue_lines(text))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(title.as_str()),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::Vending {
            cursor,
            inserted_disks,
        } => {
            render_vending_modal(frame, game, *cursor, *inserted_disks);
        }
        UiMode::ItemMenu {
            selected,
            action_idx,
        } => {
            let area = centered_rect(40, 40, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_item_menu_lines(game, *selected, *action_idx))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.item_menu")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::GroundItemMenu { action_idx } => {
            let area = centered_rect(40, 36, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_ground_item_menu_lines(game, *action_idx))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.item_menu")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::GroundSwapSelect { selected } => {
            let area = centered_rect(48, 56, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_ground_swap_lines(game, *selected))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.item_menu")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::RenameItem { selected, input } => {
            let area = centered_rect(55, 30, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_rename_lines(game, *selected, input))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ui_chrome_color(game)))
                        .title(tr("title.rename_item")),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::Crafting {
            cursor,
            selected_inv,
            focus,
            grid,
        } => {
            render_crafting_modal(frame, game, *cursor, *selected_inv, *focus, grid);
        }
        UiMode::Dead {
            scroll,
            selected_action,
        } => {
            let area = centered_rect(75, 75, frame.area());
            frame.render_widget(Clear, area);
            let container = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_chrome_color(game)))
                .title(tr("title.death"));
            let inner = container.inner(area);
            frame.render_widget(container, area);

            let panes =
                Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
                    .split(inner);

            let summary = Paragraph::new(build_dead_summary_lines(game))
                .block(
                    Block::default()
                        .borders(Borders::RIGHT)
                        .border_style(Style::default().fg(ui_chrome_color(game))),
                )
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(summary, panes[0]);

            let restart_style = if *selected_action == DeadAction::Restart {
                Style::default().fg(Color::Yellow).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let exit_style = if *selected_action == DeadAction::Exit {
                Style::default().fg(Color::Yellow).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let button_line = Line::from(vec![
                Span::styled("[ ", restart_style),
                Span::styled(tr("death.btn_restart"), restart_style),
                Span::styled(" ]  ", restart_style),
                Span::styled("[ ", exit_style),
                Span::styled(tr("death.btn_exit"), exit_style),
                Span::styled(" ]", exit_style),
            ]);
            let summary_inner = panes[0].inner(Margin {
                horizontal: 1,
                vertical: 1,
            });
            let button_y = summary_inner.height.saturating_sub(1);
            let button_area = Rect {
                x: summary_inner.x,
                y: summary_inner.y + button_y,
                width: summary_inner.width,
                height: 1,
            };
            frame.render_widget(Paragraph::new(button_line), button_area);

            let logs = Paragraph::new(build_dead_log_lines(game, *scroll))
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(logs, panes[1]);
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    let horizontal = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1]);
    horizontal[1]
}

fn is_hp_critical(game: &Game) -> bool {
    game.player_hp.max(0) * 10 <= game.player_max_hp.max(1)
}

fn ui_chrome_color(game: &Game) -> Color {
    if is_hp_critical(game) {
        Color::Red
    } else if game.player_hunger.max(0) * 10 < game.player_max_hunger.max(1) {
        Color::Yellow
    } else {
        Color::White
    }
}

fn is_hunger_critical(game: &Game) -> bool {
    game.player_hunger.max(0) * 10 <= game.player_max_hunger.max(1)
}

fn tile_blocks_sight(tile: Tile) -> bool {
    matches!(tile, Tile::Wall | Tile::Rock | Tile::Mountain | Tile::Forest)
}

fn has_line_of_sight(game: &mut Game, from_x: i32, from_y: i32, to_x: i32, to_y: i32) -> bool {
    if from_x == to_x && from_y == to_y {
        return true;
    }
    let mut x = from_x;
    let mut y = from_y;
    let dx = (to_x - from_x).abs();
    let sx = if from_x < to_x { 1 } else { -1 };
    let dy = -(to_y - from_y).abs();
    let sy = if from_y < to_y { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x == to_x && y == to_y {
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
        if x == to_x && y == to_y {
            break;
        }
        if tile_blocks_sight(game.tile(x, y)) {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy)]
struct MapLayerGlyph {
    glyph: char,
    color: Color,
    bold: bool,
}

fn build_map_lines(game: &mut Game, width: u16, height: u16) -> Vec<Line<'static>> {
    let cells_w = (width.saturating_add(1)) / 2;
    let cells_h = (height.saturating_add(1)) / 2;
    let render_w = cells_w.saturating_mul(2).saturating_sub(1);
    let render_h = cells_h.saturating_mul(2).saturating_sub(1);
    let mut lines = Vec::with_capacity(render_h as usize);
    let center_x = (cells_w / 2) as i32;
    let center_y = (cells_h / 2) as i32;
    let player_x = game.player.x;
    let player_y = game.player.y;

    let mut effect_gaps: HashMap<(u16, u16), char> = HashMap::new();
    let mut effect_targets: HashSet<(i32, i32)> = HashSet::new();
    let mut effect_cells: HashMap<(i32, i32), (char, Color, bool)> = HashMap::new();
    for cell in game.active_effect_cells() {
        effect_cells.insert((cell.x, cell.y), (cell.glyph, cell.color, cell.bold));
    }
    for fx in &game.attack_effects {
        if fx.delay_frames > 0 {
            continue;
        }
        effect_targets.insert((fx.to.x, fx.to.y));
        let from_dx = fx.from.x - game.player.x;
        let from_dy = fx.from.y - game.player.y;
        let to_dx = fx.to.x - game.player.x;
        let to_dy = fx.to.y - game.player.y;
        let mid_dx = from_dx + to_dx;
        let mid_dy = from_dy + to_dy;
        let gap_x = 2 * center_x + mid_dx;
        let gap_y = 2 * center_y + mid_dy;
        if gap_x < 0 || gap_y < 0 {
            continue;
        }
        let gx = gap_x as u16;
        let gy = gap_y as u16;
        if gx >= render_w || gy >= render_h {
            continue;
        }
        let dir_x = (fx.to.x - fx.from.x).signum();
        let dir_y = (fx.to.y - fx.from.y).signum();
        let glyph = match (dir_x, dir_y) {
            (1, 0) | (-1, 0) => '-',
            (0, 1) | (0, -1) => '|',
            (1, 1) | (-1, -1) => '\\',
            (1, -1) | (-1, 1) => '/',
            _ => '*',
        };
        effect_gaps.insert((gx, gy), glyph);
    }

    for ry in 0..render_h {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(render_w as usize);
        for rx in 0..render_w {
            let is_cell = (rx % 2 == 0) && (ry % 2 == 0);
            if !is_cell {
                if let Some(gch) = effect_gaps.get(&(rx, ry)) {
                    spans.push(Span::styled(
                        gch.to_string(),
                        Style::default().fg(Color::Yellow).bold(),
                    ));
                } else {
                    spans.push(Span::raw(" "));
                }
                continue;
            }

            let sx = rx / 2;
            let sy = ry / 2;
            let dx = sx as i32 - center_x;
            let dy = sy as i32 - center_y;
            let world_x = player_x + dx;
            let world_y = player_y + dy;
            let lit_by_torch = game.is_lit_by_torch(world_x, world_y);
            let in_radius = dx * dx + dy * dy <= VISION_RADIUS * VISION_RADIUS;
            let player_los = has_line_of_sight(game, player_x, player_y, world_x, world_y);
            let in_vision = in_radius && player_los;
            let torch_visible = lit_by_torch && player_los;
            let bright = is_bright_by_facing(game.facing, dx, dy);
            let dim_mod = if bright {
                Modifier::empty()
            } else {
                Modifier::DIM
            };

            let span = if !in_vision && !torch_visible {
                Span::raw(" ")
            } else if let Some((glyph, color, bold)) = effect_cells.get(&(world_x, world_y)) {
                let mut style = Style::default().fg(*color).add_modifier(dim_mod);
                if *bold {
                    style = style.bold();
                }
                Span::styled(glyph.to_string(), style)
            } else {
                let tile = game.tile(world_x, world_y);
                let biome_id = game.biome_id_at(world_x, world_y);
                let tile_shift = biome_brightness_shift(biome_id);

                let tile_is_opaque = !tile.walkable() || tile == Tile::Forest;

                // Layer 0: floor + pickable objects.
                let mut layer_floor: Option<MapLayerGlyph> = if tile_is_opaque {
                    None
                } else {
                    let base = if bright { tile.color() } else { shadow_color(tile) };
                    let fg = apply_brightness_shift(base, tile_shift);
                    Some(MapLayerGlyph {
                        glyph: tile.glyph(),
                        color: fg,
                        bold: false,
                    })
                };
                if game.has_blood_stain(world_x, world_y) {
                    layer_floor = Some(MapLayerGlyph {
                        glyph: '*',
                        color: Color::Indexed(52),
                        bold: true,
                    });
                }
                if game.has_torch_at(world_x, world_y) {
                    let torch_color = if bright {
                        Color::Indexed(220)
                    } else {
                        Color::Indexed(94)
                    };
                    layer_floor = Some(MapLayerGlyph {
                        glyph: 'i',
                        color: torch_color,
                        bold: true,
                    });
                }
                if game.stone_tablet_at(world_x, world_y).is_some() {
                    let tablet_color = if bright {
                        Color::Indexed(188)
                    } else {
                        Color::Indexed(102)
                    };
                    layer_floor = Some(MapLayerGlyph {
                        glyph: ']',
                        color: tablet_color,
                        bold: true,
                    });
                }
                if let Some(structure) = game.structure_at(world_x, world_y) {
                    layer_floor = Some(MapLayerGlyph {
                        glyph: structure.glyph(),
                        color: structure.color(bright),
                        bold: true,
                    });
                }
                if let Some(item) = game.item_at(world_x, world_y) {
                    let item_color = if bright {
                        item.color()
                    } else {
                        Color::Indexed(94)
                    };
                    layer_floor = Some(MapLayerGlyph {
                        glyph: item.glyph(),
                        color: item_color,
                        bold: true,
                    });
                }
                if game.copper_at(world_x, world_y).is_some() {
                    let copper_color = if bright {
                        Color::Indexed(173)
                    } else {
                        Color::Indexed(95)
                    };
                    layer_floor = Some(MapLayerGlyph {
                        glyph: 'o',
                        color: copper_color,
                        bold: true,
                    });
                }

                // Layer 1: opaque blocks + actors.
                let mut layer_solid: Option<MapLayerGlyph> = if tile_is_opaque {
                    let base = if bright { tile.color() } else { shadow_color(tile) };
                    let fg = apply_brightness_shift(base, tile_shift);
                    Some(MapLayerGlyph {
                        glyph: tile.glyph(),
                        color: fg,
                        bold: false,
                    })
                } else {
                    None
                };
                if let Some((eglyph, ecolor)) = game.enemy_visual_at(world_x, world_y) {
                    let enemy_color = if bright { ecolor } else { Color::Indexed(52) };
                    layer_solid = Some(MapLayerGlyph {
                        glyph: eglyph,
                        color: enemy_color,
                        bold: false,
                    });
                }
                if sx as i32 == center_x && sy as i32 == center_y {
                    layer_solid = Some(MapLayerGlyph {
                        glyph: '@',
                        color: Color::Red,
                        bold: true,
                    });
                }

                // Layer 2: top-most overlays.
                let layer_top: Option<MapLayerGlyph> = if effect_targets.contains(&(world_x, world_y)) {
                    Some(MapLayerGlyph {
                        glyph: '*',
                        color: Color::Yellow,
                        bold: true,
                    })
                } else {
                    None
                };

                let composed = layer_top.or(layer_solid).or(layer_floor);
                if let Some(cell) = composed {
                    let mut style = Style::default().fg(cell.color).add_modifier(dim_mod);
                    if cell.bold {
                        style = style.bold();
                    }
                    Span::styled(cell.glyph.to_string(), style)
                } else {
                    Span::raw(" ")
                }
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn build_status_lines(game: &mut Game, esc_hold_count: u8) -> Vec<Line<'static>> {
    let esc_line = if esc_hold_count == 0 {
        tr("status.hold_esc").to_string()
    } else {
        trf(
            "status.hold_esc_progress",
            &[
                ("count", esc_hold_count.to_string()),
                ("max", ESC_HOLD_STEPS.to_string()),
            ],
        )
    };

    let hp_color = if is_hp_critical(game) {
        Color::Red
    } else {
        Color::Green
    };

    vec![
        build_gauge_line(
            tr("status.hp").replace("{hp}/{max}", ""),
            game.player_hp.max(0),
            game.player_max_hp,
            14,
            hp_color,
        ),
        build_gauge_line(
            tr("status.mp").replace("{mp}/{max}", ""),
            game.player_mp.max(0),
            game.player_max_mp,
            14,
            Color::LightBlue,
        ),
        build_gauge_line(
            tr("status.hunger").replace("{v}/{max}", ""),
            game.player_hunger.max(0),
            game.player_max_hunger,
            14,
            if is_hunger_critical(game) {
                Color::Yellow
            } else {
                Color::White
            },
        ),
        build_gauge_line(
            tr("status.ancient").to_string(),
            game.ancient_charge() as i32,
            9,
            14,
            Color::Indexed(91),
        ),
        Line::from(trf(
            "status.atk_def",
            &[
                ("atk", game.player_attack_power().to_string()),
                ("def", game.player_defense().to_string()),
                ("agi", game.player_agility().to_string()),
            ],
        )),
        Line::from(trf(
            "status.effects",
            &[("list", game.player_status_summary())],
        )),
        Line::from(trf("status.level", &[("level", game.level.to_string())])),
        build_gauge_line(
            tr("status.exp").replace("{exp}/{next}", ""),
            game.exp as i32,
            game.next_exp as i32,
            14,
            Color::Yellow,
        ),
        Line::from(trf("status.turn", &[("turn", game.turn.to_string())])),
        Line::from(trf("status.floor", &[("floor", game.floor.to_string())])),
        Line::from(trf(
            "status.enemies",
            &[("count", game.enemies.len().to_string())],
        )),
        Line::from(trf(
            "status.items",
            &[
                ("count", game.inventory_len().to_string()),
                ("max", MAX_INVENTORY.to_string()),
            ],
        )),
        Line::from(trf(
            "status.copper",
            &[("grams", Game::copper_weight_text(game.player_copper_disks))],
        )),
        Line::from(trf("status.seed", &[("seed", game.world.seed.to_string())])),
        Line::from(trf(
            "status.pos",
            &[
                ("x", game.player.x.to_string()),
                ("y", game.player.y.to_string()),
            ],
        )),
        Line::from(trf(
            "status.facing",
            &[("label", game.facing.label().to_string())],
        )),
        Line::from(trf(
            "status.chunks",
            &[("count", game.generated_chunks().to_string())],
        )),
        Line::from(trf(
            "status.biome",
            &[("name", game.current_biome_name().to_string())],
        )),
        Line::from(trf("status.lang", &[("lang", current_lang().to_string())])),
        Line::raw(""),
        Line::from(tr("status.ctrl.wasd")),
        Line::from(tr("status.ctrl.diag")),
        Line::from(tr("status.ctrl.arrows")),
        Line::from(tr("status.ctrl.face")),
        Line::from(tr("status.ctrl.attack")),
        Line::from(tr("status.ctrl.num")),
        Line::from(tr("status.ctrl.menu")),
        Line::from(tr("status.ctrl.wait")),
        Line::from(esc_line),
    ]
}

fn build_gauge_line(
    label: String,
    current: i32,
    max: i32,
    width: usize,
    fill_color: Color,
) -> Line<'static> {
    let max_v = max.max(1);
    let cur_v = current.clamp(0, max_v);
    let filled = ((cur_v as i64 * width as i64) + max_v as i64 - 1) / max_v as i64;
    let filled = filled as usize;
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(width + 6);
    spans.push(Span::raw(format!("{} ", label.trim())));
    spans.push(Span::styled(
        "╶",
        if filled > 0 {
            Style::default().fg(fill_color).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        },
    ));
    for i in 0..width {
        spans.push(Span::styled(
            "─",
            if i < filled {
                Style::default().fg(fill_color).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
    }
    spans.push(Span::styled(
        "╴",
        if filled >= width {
            Style::default().fg(fill_color).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        },
    ));
    spans.push(Span::raw(format!(" {}/{}", cur_v, max_v)));
    Line::from(spans)
}

fn build_log_lines(game: &Game, max_lines: usize) -> Vec<Line<'static>> {
    if game.logs.is_empty() {
        return vec![Line::from("")];
    }
    let keep = max_lines.max(1);
    let start = game.logs.len().saturating_sub(keep);
    game.logs[start..]
        .iter()
        .map(|entry| Line::from(entry.resolve()))
        .collect()
}

fn build_quickbar_line(game: &Game) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let max_slots = 10usize;
    let equipped_sword = game.equipped_sword.as_ref();
    let equipped_shield = game.equipped_shield.as_ref();
    let equipped_accessory = game.equipped_accessory.as_ref();
    for i in 0..max_slots {
        let slot_label = if i == 9 {
            '0'
        } else {
            (b'1' + i as u8) as char
        };
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        if let Some(item) = game.inventory.get(i) {
            spans.push(Span::styled(
                format!("{}:", slot_label),
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                item.kind.glyph().to_string(),
                Style::default().fg(item.kind.color()).bold(),
            ));
            let mut equip_badges = String::new();
            if equipped_sword.is_some_and(|eq| eq.same_identity(item)) {
                equip_badges.push('W');
            }
            if equipped_shield.is_some_and(|eq| eq.same_identity(item)) {
                equip_badges.push('S');
            }
            if equipped_accessory.is_some_and(|eq| eq.same_identity(item)) {
                equip_badges.push('A');
            }
            if !equip_badges.is_empty() {
                spans.push(Span::styled(
                    format!("[{}]", equip_badges),
                    Style::default().fg(Color::Cyan).bold(),
                ));
            }
            if item.qty > 1 {
                spans.push(Span::raw(format!("x{}", item.qty)));
            }
        } else {
            spans.push(Span::styled(
                format!("{}:·", slot_label),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    Line::from(spans)
}

fn build_inventory_lines(game: &Game, selected: usize, move_selected: bool) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(trf(
            "inventory.header",
            &[
                ("count", game.inventory_len().to_string()),
                ("max", MAX_INVENTORY.to_string()),
            ],
        )),
        Line::from(if move_selected {
            tr("inventory.help_move")
        } else {
            tr("inventory.help")
        }),
        Line::from(tr("inventory.equip_legend")),
        Line::raw(""),
    ];
    if game.inventory.is_empty() {
        lines.push(Line::from(tr("inventory.empty")));
        return lines;
    }
    let equipped_sword = game.equipped_sword.as_ref();
    let equipped_shield = game.equipped_shield.as_ref();
    let equipped_accessory = game.equipped_accessory.as_ref();
    for (idx, item) in game.inventory.iter().enumerate() {
        let marker_style = if idx == selected {
            Style::default().fg(Color::Yellow).bold()
        } else {
            Style::default()
        };
        let mut equip_badges = String::new();
        if equipped_sword.is_some_and(|eq| eq.same_identity(item)) {
            equip_badges.push('W');
        }
        if equipped_shield.is_some_and(|eq| eq.same_identity(item)) {
            equip_badges.push('S');
        }
        if equipped_accessory.is_some_and(|eq| eq.same_identity(item)) {
            equip_badges.push('A');
        }
        let badge_span = if equip_badges.is_empty() {
            Span::raw("")
        } else {
            Span::styled(
                format!(" [{}]", equip_badges),
                Style::default().fg(Color::Cyan).bold(),
            )
        };
        let row_indent = move_selected && idx == selected;
        lines.push(Line::from(vec![
            Span::styled(if idx == selected { ">" } else { " " }, marker_style),
            Span::raw(" "),
            Span::raw(if row_indent { "  " } else { "" }),
            Span::styled(
                item.kind.glyph().to_string(),
                Style::default().fg(item.kind.color()).bold(),
            ),
            Span::raw(" "),
            Span::raw(item.display_name_with_qty()),
            badge_span,
        ]));
    }
    lines
}

fn build_inventory_detail_lines(game: &Game, selected: usize) -> Vec<Line<'static>> {
    let Some(item) = game.inventory.get(selected) else {
        return vec![
            Line::from(tr("inventory.no_selected")),
            Line::raw(""),
            Line::from(tr("inventory.no_selected_help")),
        ];
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                item.kind.glyph().to_string(),
                Style::default().fg(item.kind.color()).bold(),
            ),
            Span::raw(" "),
            Span::styled(item.display_name_with_qty(), Style::default().bold()),
        ]),
        Line::raw(""),
        Line::from(trf(
            "inventory.type",
            &[("type", localized_item_status(item.kind))],
        )),
        Line::from(trf("inventory.id", &[("id", item.kind.key().to_string())])),
        Line::raw(""),
        Line::from(tr("inventory.description")),
    ];
    for desc_line in localized_item_description(item.kind).lines() {
        lines.push(styled_description_line(item.kind, desc_line));
    }
    lines
}

fn styled_description_line(kind: Item, text: &str) -> Line<'static> {
    if kind != Item::ForgeScroll {
        return Line::from(text.to_string());
    }
    let cells: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let is_pattern_line = cells.len() == 3
        && cells
            .iter()
            .all(|c| matches!(*c, 'w' | 'W' | 's' | 'S' | '@' | '.'));
    if !is_pattern_line {
        return Line::from(text.to_string());
    }
    let wood_style = Style::default().fg(item_meta(Item::Wood).color).bold();
    let stone_style = Style::default().fg(item_meta(Item::Stone).color).bold();
    let center_style = Style::default().fg(creature_meta("player").color).bold();
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let span = match ch {
            'w' | 'W' => Span::styled(ch.to_string(), wood_style),
            's' | 'S' => Span::styled(ch.to_string(), stone_style),
            '@' => Span::styled(ch.to_string(), center_style),
            _ => Span::raw(ch.to_string()),
        };
        spans.push(span);
    }
    Line::from(spans)
}

fn render_inventory_modal(frame: &mut Frame, game: &Game, selected: usize, move_selected: bool) {
    let area = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, area);
    let container = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_chrome_color(game)))
        .title(tr("title.inventory"));
    let inner = container.inner(area);
    frame.render_widget(container, area);

    let cols =
        Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).split(inner);
    let left = Paragraph::new(build_inventory_lines(game, selected, move_selected))
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(ui_chrome_color(game))),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(left, cols[0]);

    let right = Paragraph::new(build_inventory_detail_lines(game, selected))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(right, cols[1]);
}

fn build_main_menu_lines(selected: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(tr("menu.help_updown")),
        Line::from(tr("menu.help_enter")),
        Line::from(tr("menu.help_esc")),
        Line::raw(""),
    ];
    for (idx, entry) in MAIN_MENU_ENTRIES.iter().enumerate() {
        let marker = if idx == selected { ">" } else { " " };
        lines.push(Line::from(format!("{} {}", marker, entry.label())));
    }
    lines
}

fn build_debug_console_lines(
    input: &str,
    suggestion_idx: usize,
    max_lines: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!("{}{}", tr("debug.prompt"), input))];
    let suggestions = debug_console_suggestions(input);
    if !suggestions.is_empty() {
        let selected = suggestion_idx.min(suggestions.len().saturating_sub(1));
        let fixed_lines = 3usize;
        let mut start = 0usize;
        let mut end = suggestions.len();
        loop {
            let top_ellipsis = usize::from(start > 0);
            let bottom_ellipsis = usize::from(end < suggestions.len());
            let available_window = max_lines
                .saturating_sub(fixed_lines + top_ellipsis + bottom_ellipsis)
                .max(1);
            if selected < start {
                start = selected;
            }
            if selected >= start + available_window {
                start = selected + 1 - available_window;
            }
            end = (start + available_window).min(suggestions.len());
            let new_top = usize::from(start > 0);
            let new_bottom = usize::from(end < suggestions.len());
            let total = fixed_lines + new_top + new_bottom + (end - start);
            if total <= max_lines {
                break;
            }
            if end > start {
                end -= 1;
            } else {
                break;
            }
        }
        let top_ellipsis = start > 0;
        let bottom_ellipsis = end < suggestions.len();
        lines.push(Line::raw(""));
        lines.push(Line::from(tr("debug.candidates")));
        if top_ellipsis {
            lines.push(Line::from("  ..."));
        }
        for (idx, candidate) in suggestions.iter().enumerate().skip(start).take(end - start) {
            let style = if idx == selected {
                Style::default()
                    .bg(Color::Indexed(60))
                    .fg(Color::White)
                    .bold()
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(format!("  {candidate}"), style)));
        }
        if bottom_ellipsis {
            lines.push(Line::from("  ..."));
        }
    }
    lines
}

fn debug_console_suggestions(input: &str) -> Vec<String> {
    let trimmed = input.trim_start();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let ends_with_space = trimmed.ends_with(' ');

    let top_level = [
        "give",
        "tp",
        "find_exit",
        "inv",
        "floor",
        "place",
    ];

    if parts.is_empty() {
        return top_level.iter().map(|s| (*s).to_string()).collect();
    }

    if parts.len() == 1 && !ends_with_space {
        let needle = parts[0].to_ascii_lowercase();
        return top_level
            .iter()
            .filter(|cmd| cmd.starts_with(&needle))
            .map(|s| (*s).to_string())
            .collect();
    }

    let cmd = parts[0].to_ascii_lowercase();
    let arg_index = if ends_with_space {
        parts.len()
    } else {
        parts.len().saturating_sub(1)
    };
    let current = if ends_with_space {
        ""
    } else {
        parts.last().copied().unwrap_or_default()
    }
    .to_ascii_lowercase();

    match cmd.as_str() {
        "give" => {
            if arg_index == 1 {
                let mut items: Vec<String> = defs()
                    .items
                    .iter()
                    .map(|(id, _)| id.clone())
                    .filter(|id: &String| id.starts_with(&current))
                    .collect();
                items.sort();
                items.truncate(16);
                items
            } else {
                Vec::new()
            }
        }
        "tp" => {
            if arg_index == 1 {
                ["exit", "ruin", "altar", "temple", "substory"]
                    .into_iter()
                    .filter(|s| s.starts_with(&current))
                    .map(|s| s.to_string())
                    .collect()
            } else {
                Vec::new()
            }
        }
        "inv" => {
            if arg_index == 1 {
                ["on", "off", "toggle"]
                    .into_iter()
                    .filter(|s| s.starts_with(&current))
                    .map(|s| s.to_string())
                    .collect()
            } else {
                Vec::new()
            }
        }
        "place" => {
            if arg_index == 1 {
                [
                    "tree", "rock", "tablet", "altar", "temple", "terminal", "vending", "bone",
                    "cable",
                ]
                .into_iter()
                .filter(|s| s.starts_with(&current))
                .map(|s| s.to_string())
                .collect()
            } else if parts.get(1).is_some_and(|p| p.eq_ignore_ascii_case("tablet")) && arg_index == 2 {
                ["mercy", "might", "oracle"]
                    .into_iter()
                    .filter(|s| s.starts_with(&current))
                    .map(|s| s.to_string())
                    .collect()
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn apply_debug_suggestion(input: &str, suggestion_idx: usize) -> Option<String> {
    let suggestions = debug_console_suggestions(input);
    let suggestion = suggestions.get(suggestion_idx)?.clone();
    let trimmed_start = input.trim_start();
    let leading_ws_len = input.len().saturating_sub(trimmed_start.len());
    let leading_ws = &input[..leading_ws_len];
    let parts: Vec<&str> = trimmed_start.split_whitespace().collect();
    let ends_with_space = trimmed_start.ends_with(' ');

    if parts.is_empty() {
        return Some(format!("{leading_ws}{suggestion} "));
    }

    if parts.len() == 1 && !ends_with_space {
        return Some(format!("{leading_ws}{suggestion} "));
    }

    let cmd = parts[0];
    if ends_with_space {
        return Some(format!("{input}{suggestion} "));
    }

    if let Some(prefix) = input.rfind(parts.last().copied().unwrap_or_default()) {
        let mut out = String::with_capacity(input.len() + suggestion.len() + 1);
        out.push_str(&input[..prefix]);
        out.push_str(cmd);
        if parts.len() > 1 {
            out.clear();
            out.push_str(leading_ws);
            out.push_str(cmd);
            out.push(' ');
            for part in parts.iter().skip(1).take(parts.len().saturating_sub(2)) {
                out.push_str(part);
                out.push(' ');
            }
            out.push_str(&suggestion);
            out.push(' ');
            return Some(out);
        }
        out.push_str(&suggestion);
        out.push(' ');
        return Some(out);
    }

    None
}

fn execute_debug_command(game: &mut Game, command: &str) {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return;
    }
    let mut parts = trimmed.split_whitespace();
    let cmd = parts.next().unwrap_or_default();
    let cmd_l = cmd.to_ascii_lowercase();
    match cmd_l.as_str() {
        "give" | "付与" => {
            let Some(item_key) = parts.next() else {
                game.push_log_tr("debug.give_usage");
                return;
            };
            let requested = parts
                .next()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(1)
                .max(1);
            let Some(kind) = Item::from_key(item_key) else {
                game.push_log_trf(
                    "debug.give_unknown_item",
                    &[("item", item_key.to_string())],
                );
                return;
            };
            let mut added: u16 = 0;
            for _ in 0..requested {
                if game.add_item_kind_to_inventory(kind) {
                    added = added.saturating_add(1);
                } else {
                    break;
                }
            }
            if added == requested {
                game.push_log_trf(
                    "debug.give_ok",
                    &[
                        ("item", log_arg_item_ref(kind)),
                        ("count", added.to_string()),
                    ],
                );
            } else {
                game.push_log_trf(
                    "debug.give_partial",
                    &[
                        ("item", log_arg_item_ref(kind)),
                        ("added", added.to_string()),
                        ("requested", requested.to_string()),
                    ],
                );
            }
        }
        "tp" => {
            let Some(arg1) = parts.next() else {
                game.push_log_tr("debug.tp_usage");
                return;
            };
            if arg1.eq_ignore_ascii_case("exit")
                || arg1.eq_ignore_ascii_case("stairs")
                || arg1 == "出口"
            {
                if let Some(pos) = game.find_nearest_stairs(512) {
                    match game.teleport_player(pos.x, pos.y) {
                        Ok(()) => game.push_log_trf(
                            "debug.tp_ok",
                            &[("x", pos.x.to_string()), ("y", pos.y.to_string())],
                        ),
                        Err(msg) => game.push_log(msg),
                    }
                } else {
                    game.push_log_tr("debug.exit_not_found");
                }
                return;
            }
            if arg1.eq_ignore_ascii_case("altar")
                || arg1.eq_ignore_ascii_case("temple")
                || arg1.eq_ignore_ascii_case("ruin")
                || arg1.eq_ignore_ascii_case("substory")
                || arg1 == "祭壇"
                || arg1 == "寺院"
                || arg1 == "遺跡"
            {
                let kinds: &[crate::game::StructureKind] = match arg1.to_ascii_lowercase().as_str()
                {
                    "altar" => &[crate::game::StructureKind::Altar],
                    "temple" => &[crate::game::StructureKind::TempleCore],
                    "substory" => &[crate::game::StructureKind::SubstoryCore],
                    _ => &[
                        crate::game::StructureKind::Altar,
                        crate::game::StructureKind::TempleCore,
                        crate::game::StructureKind::SubstoryCore,
                    ],
                };
                if let Some(pos) = game.find_nearest_structure_approach(kinds, 512) {
                    match game.teleport_player(pos.x, pos.y) {
                        Ok(()) => game.push_log_trf(
                            "debug.tp_ok",
                            &[("x", pos.x.to_string()), ("y", pos.y.to_string())],
                        ),
                        Err(msg) => game.push_log(msg),
                    }
                } else {
                    game.push_log_tr("debug.ruin_not_found");
                }
                return;
            }
            let Some(arg2) = parts.next() else {
                game.push_log_tr("debug.tp_usage");
                return;
            };
            let Ok(x) = arg1.parse::<i32>() else {
                game.push_log_tr("debug.tp_usage");
                return;
            };
            let Ok(y) = arg2.parse::<i32>() else {
                game.push_log_tr("debug.tp_usage");
                return;
            };
            match game.teleport_player(x, y) {
                Ok(()) => game.push_log_trf(
                    "debug.tp_ok",
                    &[("x", x.to_string()), ("y", y.to_string())],
                ),
                Err(msg) => game.push_log(msg),
            }
        }
        "find_exit" | "exit_search" | "出口探索" => {
            if let Some(pos) = game.find_nearest_stairs(512) {
                game.push_log_trf(
                    "debug.exit_found",
                    &[("x", pos.x.to_string()), ("y", pos.y.to_string())],
                );
            } else {
                game.push_log_tr("debug.exit_not_found");
            }
        }
        "inv" | "god" | "invincible" | "無敵" => {
            let mode = parts.next().map(|s| s.to_ascii_lowercase());
            match mode.as_deref() {
                None | Some("toggle") => {
                    let next = !game.invincible();
                    game.set_invincible(next);
                }
                Some("on") => game.set_invincible(true),
                Some("off") => game.set_invincible(false),
                Some(_) => {
                    game.push_log_tr("debug.inv_usage");
                    return;
                }
            }
            game.push_log(if game.invincible() {
                tr("debug.inv_on")
            } else {
                tr("debug.inv_off")
            });
        }
        "floor" | "fl" | "階層" => {
            let Some(arg) = parts.next() else {
                game.push_log_tr("debug.floor_usage");
                return;
            };
            let Ok(target_floor) = arg.parse::<u32>() else {
                game.push_log_tr("debug.floor_usage");
                return;
            };
            if target_floor < 1 {
                game.push_log_tr("debug.floor_usage");
                return;
            }
            let current_floor = game.floor;
            if target_floor == current_floor {
                game.push_log_trf(
                    "debug.floor_ok",
                    &[("floor", current_floor.to_string())],
                );
                return;
            }
            if target_floor < current_floor {
                game.push_log_trf(
                    "debug.floor_cannot_ascend",
                    &[
                        ("current", current_floor.to_string()),
                        ("target", target_floor.to_string()),
                    ],
                );
                return;
            }
            for _ in current_floor..target_floor {
                game.descend_floor();
            }
            game.push_log_trf(
                "debug.floor_ok",
                &[("floor", game.floor.to_string())],
            );
        }
        "place" | "obj" | "構造物" => {
            let Some(kind) = parts.next().map(|s| s.to_ascii_lowercase()) else {
                game.push_log_tr("debug.place_usage");
                return;
            };
            let placed = match kind.as_str() {
                "tree" | "forest" | "木" => game
                    .debug_place_tile_ahead(Tile::Forest)
                    .map(|(x, y)| (tr("debug.place.tree").to_string(), x, y)),
                "rock" | "stone" | "岩" => game
                    .debug_place_tile_ahead(Tile::Rock)
                    .map(|(x, y)| (tr("debug.place.rock").to_string(), x, y)),
                "tablet" | "slab" | "石板" | "石碑" => {
                    let tablet = match parts.next().map(|s| s.to_ascii_lowercase()) {
                        None => StoneTabletKind::Oracle,
                        Some(v) if v == "mercy" || v == "oriens" => StoneTabletKind::Mercy,
                        Some(v) if v == "might" || v == "occasus" => StoneTabletKind::Might,
                        Some(v) if v == "oracle" || v == "prophecy" => StoneTabletKind::Oracle,
                        Some(_) => {
                            game.push_log_tr("debug.place_usage");
                            return;
                        }
                    };
                    game.debug_place_tablet_ahead(tablet)
                        .map(|(x, y)| (tr("debug.place.tablet").to_string(), x, y))
                }
                "altar" | "祭壇" => game
                    .debug_place_structure_ahead(StructureKind::Altar)
                    .map(|(x, y)| (tr("debug.place.altar").to_string(), x, y)),
                "temple" | "temple_core" | "寺院" => game
                    .debug_place_temple_ahead()
                    .map(|(x, y)| (tr("debug.place.temple_core").to_string(), x, y)),
                "terminal" | "端末" => game
                    .debug_place_structure_ahead(StructureKind::Terminal)
                    .map(|(x, y)| (tr("debug.place.terminal").to_string(), x, y)),
                "vending" | "vending_machine" | "自販機" | "自動販売機" => game
                    .debug_place_structure_ahead(StructureKind::VendingMachine)
                    .map(|(x, y)| (tr("debug.place.vending_machine").to_string(), x, y)),
                "bone" | "bone_rack" | "骨棚" => game
                    .debug_place_structure_ahead(StructureKind::BoneRack)
                    .map(|(x, y)| (tr("debug.place.bone_rack").to_string(), x, y)),
                "cable" | "cable_pylon" | "導線柱" => game
                    .debug_place_structure_ahead(StructureKind::CablePylon)
                    .map(|(x, y)| (tr("debug.place.cable_pylon").to_string(), x, y)),
                _ => {
                    game.push_log_tr("debug.place_usage");
                    return;
                }
            };
            match placed {
                Ok((label, x, y)) => game.push_log_trf(
                    "debug.place_ok",
                    &[
                        ("object", label),
                        ("x", x.to_string()),
                        ("y", y.to_string()),
                    ],
                ),
                Err(msg) => game.push_log(msg),
            }
        }
        _ => {
            game.push_log_trf("debug.unknown", &[("cmd", trimmed.to_string())]);
        }
    }
}

fn build_stairs_prompt_lines(selected_action: StairsAction) -> Vec<Line<'static>> {
    let descend_style = if selected_action == StairsAction::Descend {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let stay_style = if selected_action == StairsAction::Stay {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    vec![
        Line::from(tr("stairs.prompt")),
        Line::raw(""),
        Line::from(vec![
            Span::styled("[ ", descend_style),
            Span::styled(tr("stairs.option.descend"), descend_style),
            Span::styled(" ]", descend_style),
        ]),
        Line::from(vec![
            Span::styled("[ ", stay_style),
            Span::styled(tr("stairs.option.stay"), stay_style),
            Span::styled(" ]", stay_style),
        ]),
        Line::raw(""),
        Line::from(tr("stairs.help")),
    ]
}

fn build_settings_lines(selected: usize) -> Vec<Line<'static>> {
    let langs = available_languages();
    let mut lines = vec![
        Line::from(tr("settings.language_title")),
        Line::from(tr("settings.language_help")),
        Line::raw(""),
    ];
    for (idx, (code, name)) in langs.iter().enumerate() {
        let marker = if idx == selected { ">" } else { " " };
        let current = if *code == current_lang() {
            tr("settings.current_mark")
        } else {
            ""
        };
        let localized_name = tr_or_fallback(format!("lang.name.{code}"), name);
        lines.push(Line::from(format!(
            "{marker} {localized_name} ({code}) {current}"
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(tr("settings.back")));
    lines
}

fn build_title_menu_lines(selected: usize, _has_save: bool) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("")];
    for raw in TITLE_LOGO.lines().filter(|line| !line.trim().is_empty()) {
        lines.push(Line::from(Span::styled(
            raw.to_string(),
            Style::default().fg(Color::Indexed(54)),
        )));
    }
    lines.push(Line::from(""));
    let menu_width = TITLE_MENU_ENTRIES
        .iter()
        .map(|entry| entry.label().chars().count())
        .max()
        .unwrap_or(0)
        + 4;
    for (idx, entry) in TITLE_MENU_ENTRIES.iter().enumerate() {
        let is_selected = idx == selected;
        let disabled = false;
        let style = if disabled {
            Style::default().fg(Color::DarkGray)
        } else if is_selected {
            Style::default()
                .fg(Color::Indexed(230))
                .bg(Color::Indexed(60))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Indexed(188))
        };
        let label = entry.label().to_string();
        lines.push(Line::from(Span::styled(
            format!("{label:^width$}", width = menu_width),
            style,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        tr("title_screen.help"),
        Style::default().fg(Color::White),
    )));
    lines
}

fn save_file_path(slot: usize) -> PathBuf {
    PathBuf::from(format!("{}_{}.json", SAVE_FILE_BASENAME, slot + 1))
}

fn has_save_in_slot(slot: usize) -> bool {
    save_file_path(slot).exists()
}

fn has_save_file() -> bool {
    (0..SAVE_SLOT_COUNT).any(has_save_in_slot)
}

fn build_title_slot_lines(selected: usize, _for_load: bool) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(""), Line::from("")];
    for slot in 0..SAVE_SLOT_COUNT {
        let has_save = has_save_in_slot(slot);
        let disabled = false;
        let style = if disabled {
            Style::default().fg(Color::DarkGray)
        } else if slot == selected {
            Style::default()
                .fg(Color::Indexed(230))
                .bg(Color::Indexed(60))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Indexed(188))
        };
        let status = if has_save {
            tr("title_slots.used")
        } else {
            tr("title_slots.new")
        };
        lines.push(Line::from(Span::styled(
            format!("  {} {}  {}", tr("title_slots.slot"), slot + 1, status),
            style,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        tr("title_slots.help"),
        Style::default().fg(Color::White),
    )));
    lines
}

fn build_save_preview_lines(game: &mut Game) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            format!("{} {}", tr("title_slots.floor"), game.floor),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{} {}", tr("title_slots.score"), game.stat_total_exp),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!("HP {}/{}   MP {}/{}", game.player_hp, game.player_max_hp, game.player_mp, game.player_max_mp),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            tr("title_slots.items"),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
    ];

    let mut item_spans: Vec<Span<'static>> = Vec::new();
    if game.inventory.is_empty() {
        item_spans.push(Span::styled(
            tr("title_slots.empty"),
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for item in game.inventory.iter().take(10) {
            item_spans.push(Span::styled(
                format!("{} ", item.kind.glyph()),
                Style::default().fg(item.kind.color()),
            ));
        }
    }
    lines.push(Line::from(item_spans));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        tr("title_slots.area"),
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )));

    for dy in -2..=2 {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for dx in -2..=2 {
            let x = game.player.x + dx;
            let y = game.player.y + dy;
            let in_radius = dx * dx + dy * dy <= VISION_RADIUS * VISION_RADIUS;
            let visible = in_radius && has_line_of_sight(game, game.player.x, game.player.y, x, y);
            if !visible {
                spans.push(Span::raw("  "));
                continue;
            }
            let mut glyph = game.tile(x, y).glyph();
            let mut color = game.tile(x, y).color();
            if let Some(item) = game.item_at(x, y) {
                glyph = item.glyph();
                color = item.color();
            }
            if game.stone_tablet_at(x, y).is_some() {
                glyph = ']';
                color = Color::Indexed(188);
            }
            if let Some(structure) = game.structure_at(x, y) {
                glyph = structure.glyph();
                color = structure.color(true);
            }
            if let Some((eglyph, ecolor)) = game.enemy_visual_at(x, y) {
                glyph = eglyph;
                color = ecolor;
            }
            if x == game.player.x && y == game.player.y {
                glyph = '@';
                color = Color::Red;
            }
            spans.push(Span::styled(format!("{glyph} "), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn build_empty_save_preview_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            tr("title_slots.preview_empty"),
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

fn build_error_save_preview_lines(err: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            tr("title_slots.preview_error"),
            Style::default().fg(Color::Indexed(203)).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            err.to_string(),
            Style::default().fg(Color::Indexed(180)),
        )),
    ]
}

fn build_title_delete_confirm_lines(selected: usize) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            trf("title_slots.delete_confirm", &[("slot", (selected + 1).to_string())]),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            tr("title_slots.delete_help"),
            Style::default().fg(Color::White),
        )),
    ]
}

fn title_text_test_body(nonce: u64) -> String {
    const JA_HIRA: &[&str] = &[
        "あわい", "ひかり", "しずく", "ゆらぎ", "ねむり", "かぜ", "つち", "いのり",
    ];
    const ROMA: &[&str] = &[
        "oriens", "occasus", "nadir", "zenith", "septentrio", "meridies", "abyssus", "lumen",
    ];
    const CUNEIFORM: &[&str] = &[
        "𒀭𒌓", "𒀭𒄈", "𒈫𒂊𒉈", "𒁀𒀭𒁺", "𒐊𒄰", "𒇽𒌑𒊏", "𒆠𒋫", "𒊩𒌆𒄀𒂵",
    ];
    let lang = current_lang();
    let mut out: Vec<String> = Vec::new();
    let is_ja = lang == "ja";
    for row in 0..6_u64 {
        let a = ((nonce.wrapping_add(row * 17)) % ROMA.len() as u64) as usize;
        let b = ((nonce.wrapping_add(row * 31 + 7)) % CUNEIFORM.len() as u64) as usize;
        if is_ja {
            let c = ((nonce.wrapping_add(row * 23 + 3)) % JA_HIRA.len() as u64) as usize;
            out.push(format!("{}  {}  {}", JA_HIRA[c], ROMA[a], CUNEIFORM[b]));
        } else {
            let d = ((nonce.wrapping_add(row * 29 + 11)) % ROMA.len() as u64) as usize;
            out.push(format!("{}  {}  {}", ROMA[a], ROMA[d], CUNEIFORM[b]));
        }
    }
    out.join("\n")
}

fn build_title_text_test_lines(nonce: u64) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("")];
    for line in title_text_test_body(nonce).lines() {
        lines.push(Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(Color::Indexed(216)),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        tr("title_text_test.help"),
        Style::default().fg(Color::White),
    )));
    lines
}

fn render_title_screen(frame: &mut Frame, game: &Game, selected: usize, has_save: bool) {
    let bg = Block::default().style(Style::default().bg(Color::Indexed(16)));
    frame.render_widget(bg, frame.area());

    let title_area = centered_rect(86, 72, frame.area());
    frame.render_widget(Clear, title_area);
    let widget = Paragraph::new(build_title_menu_lines(selected, has_save))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_chrome_color(game)))
                .title(tr("title.title_screen")),
        )
        .wrap(ratatui::widgets::Wrap { trim: false })
        .alignment(Alignment::Center);
    frame.render_widget(widget, title_area);
}

fn render_title_backdrop(frame: &mut Frame, game: &Game) {
    let bg = Block::default().style(Style::default().bg(Color::Indexed(16)));
    frame.render_widget(bg, frame.area());

    let area = centered_rect(86, 72, frame.area());
    frame.render_widget(Clear, area);
    let mut lines = vec![Line::from("")];
    for raw in TITLE_LOGO.lines().filter(|line| !line.trim().is_empty()) {
        lines.push(Line::from(Span::styled(
            raw.to_string(),
            Style::default().fg(Color::Indexed(54)),
        )));
    }
    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_chrome_color(game)))
                .title(tr("title.title_screen")),
        )
        .alignment(Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_title_slot_screen(frame: &mut Frame, game: &Game, selected: usize, for_load: bool) {
    render_title_backdrop(frame, game);
    let area = centered_rect(72, 42, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_chrome_color(game)))
        .title(if for_load {
            tr("title_slots.load_title")
        } else {
            tr("title_slots.start_title")
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let panes = Layout::horizontal([Constraint::Length(24), Constraint::Min(24)]).split(inner);

    let list_widget = Paragraph::new(build_title_slot_lines(selected, for_load))
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(ui_chrome_color(game))),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(list_widget, panes[0]);

    let preview_lines = if !has_save_in_slot(selected) {
        build_empty_save_preview_lines()
    } else {
        match load_game_from_slot(selected) {
            Ok(mut preview) => build_save_preview_lines(&mut preview),
            Err(err) => build_error_save_preview_lines(&err),
        }
    };
    let preview_widget = Paragraph::new(preview_lines)
        .block(
            Block::default()
                .title(tr("title_slots.preview"))
                .border_style(Style::default().fg(ui_chrome_color(game))),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(preview_widget, panes[1]);
}

fn render_title_text_test(frame: &mut Frame, game: &Game, nonce: u64) {
    render_title_backdrop(frame, game);
    let area = centered_rect(72, 54, frame.area());
    frame.render_widget(Clear, area);
    let widget = Paragraph::new(build_title_text_test_lines(nonce))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_chrome_color(game)))
                .title(tr("title_text_test.title")),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn vending_selected_product(cursor: usize) -> Option<VendingProduct> {
    VENDING_PRODUCTS.get(cursor).copied()
}

fn move_vending_cursor(cursor: usize, dx: i32, dy: i32) -> usize {
    if cursor == 12 {
        return if dy < 0 { 10 } else { 12 };
    }
    let x = (cursor % 3) as i32;
    let y = (cursor / 3) as i32;
    if dy > 0 && y == 3 {
        return 12;
    }
    let nx = (x + dx).clamp(0, 2);
    let ny = (y + dy).clamp(0, 3);
    (ny * 3 + nx) as usize
}

fn build_vending_grid_lines(cursor: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in 0..4 {
        let mut spans = Vec::new();
        for col in 0..3 {
            let idx = row * 3 + col;
            let product = VENDING_PRODUCTS[idx];
            let selected = cursor == idx;
            let style = if selected {
                Style::default()
                    .bg(Color::Indexed(60))
                    .fg(Color::White)
                    .bold()
            } else {
                Style::default()
                    .fg(item_meta(product.item).color)
                    .bold()
            };
            spans.push(Span::styled(
                format!(" {} {}as ", product.item.glyph(), product.price_as),
                style,
            ));
            if col < 2 {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    let insert_style = if cursor == 12 {
        Style::default()
            .bg(Color::Indexed(60))
            .fg(Color::White)
            .bold()
    } else {
        Style::default().fg(Color::White)
    };
    lines.push(Line::from(Span::styled(
        format!(" {} ", tr("vending.insert")),
        insert_style,
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(tr("vending.help")));
    lines
}

fn build_vending_detail_lines(game: &Game, cursor: usize, inserted_disks: u32) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(trf(
            "vending.credit",
            &[("value", inserted_disks.to_string())],
        )),
        Line::from(trf(
            "vending.held_mass",
            &[("grams", Game::copper_weight_text(game.player_copper_disks))],
        )),
        Line::raw(""),
    ];

    if cursor == 12 {
        lines.push(Line::from(tr("vending.insert")));
        lines.push(Line::raw(""));
        lines.push(Line::from(tr("vending.insert_desc")));
        return lines;
    }

    let Some(product) = vending_selected_product(cursor) else {
        return lines;
    };
    lines.push(Line::from(localized_item_name(product.item)));
    lines.push(Line::from(trf(
        "vending.price",
        &[("value", product.price_as.to_string())],
    )));
    lines.push(Line::from(localized_item_status(product.item)));
    lines.push(Line::raw(""));
    for line in localized_item_description(product.item).lines() {
        lines.push(Line::from(line.to_string()));
    }
    lines
}

fn render_vending_modal(frame: &mut Frame, game: &Game, cursor: usize, inserted_disks: u32) {
    let area = centered_rect(76, 58, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_chrome_color(game)))
        .title(tr("title.vending"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let panes = Layout::horizontal([Constraint::Length(28), Constraint::Min(28)]).split(inner);

    let left = Paragraph::new(build_vending_grid_lines(cursor))
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(ui_chrome_color(game))),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(left, panes[0]);

    let right = Paragraph::new(build_vending_detail_lines(game, cursor, inserted_disks))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(right, panes[1]);
}

fn build_hints_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(tr("hints.title")),
        Line::raw(""),
        Line::from(tr("hints.1")),
        Line::from(tr("hints.2")),
        Line::from(tr("hints.3")),
        Line::from(tr("hints.4")),
        Line::from(tr("hints.5")),
        Line::from(tr("hints.6")),
        Line::raw(""),
        Line::from(tr("hints.back")),
    ]
}

fn build_dialogue_lines(text: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = text
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect();
    if lines.is_empty() {
        lines.push(Line::from(String::new()));
    }
    lines.push(Line::from(String::new()));
    lines.push(Line::from(tr("dialogue.help").to_string()));
    lines
}

fn build_dead_summary_lines(game: &Game) -> Vec<Line<'static>> {
    vec![
        Line::from(tr("death.header")),
        Line::from(tr("death.help")),
        Line::raw(""),
        Line::from(trf("death.cause", &[("v", game.death_cause_text())])),
        Line::from(trf("death.stat.turn", &[("v", game.turn.to_string())])),
        Line::from(trf("death.stat.level", &[("v", game.level.to_string())])),
        Line::from(trf(
            "death.stat.exp_total",
            &[("v", game.stat_total_exp.to_string())],
        )),
        Line::from(trf(
            "death.stat.defeated",
            &[("v", game.stat_enemies_defeated.to_string())],
        )),
        Line::from(trf(
            "death.stat.damage_dealt",
            &[("v", game.stat_damage_dealt.to_string())],
        )),
        Line::from(trf(
            "death.stat.damage_taken",
            &[("v", game.stat_damage_taken.to_string())],
        )),
        Line::from(trf(
            "death.stat.steps",
            &[("v", game.stat_steps.to_string())],
        )),
        Line::from(trf(
            "death.stat.items",
            &[("v", game.stat_items_picked.to_string())],
        )),
    ]
}

fn build_dead_log_lines(game: &Game, scroll: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(tr("death.log_title")), Line::raw("")];
    if game.logs.is_empty() {
        lines.push(Line::from(tr("inventory.empty")));
        return lines;
    }
    let start = scroll.min(game.logs.len().saturating_sub(1));
    lines.extend(game.logs[start..].iter().map(|entry| Line::from(entry.resolve())));
    lines
}

fn build_item_menu_lines(game: &Game, selected: usize, action_idx: usize) -> Vec<Line<'static>> {
    let item_name = game
        .inventory_item_name(selected)
        .unwrap_or_else(|| tr("status.none").to_string());
    let mut lines = vec![
        Line::from(trf("item_menu.item", &[("name", item_name)])),
        Line::from(tr("item_menu.help")),
        Line::raw(""),
    ];
    for (idx, action) in ITEM_MENU_ACTIONS.iter().enumerate() {
        let marker = if idx == action_idx { ">" } else { " " };
        lines.push(Line::from(format!("{} {}", marker, action.label())));
    }
    lines
}

fn build_ground_item_menu_lines(game: &Game, action_idx: usize) -> Vec<Line<'static>> {
    let item_name = game
        .ground_item_at_player()
        .map(localized_item_name)
        .unwrap_or_else(|| tr("status.none").to_string());
    let mut lines = vec![
        Line::from(trf("item_menu.item", &[("name", item_name)])),
        Line::from(tr("ground_menu.help")),
        Line::raw(""),
    ];
    for (idx, action) in GROUND_ITEM_MENU_ACTIONS.iter().enumerate() {
        let marker = if idx == action_idx { ">" } else { " " };
        lines.push(Line::from(format!("{} {}", marker, action.label())));
    }
    lines
}

fn build_ground_swap_lines(game: &Game, selected: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(tr("ground_swap.help")), Line::raw("")];
    for (idx, item) in game.inventory.iter().enumerate() {
        let marker_style = if idx == selected {
            Style::default().fg(Color::Yellow).bold()
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(if idx == selected { ">" } else { " " }, marker_style),
            Span::raw(" "),
            Span::styled(
                item.kind.glyph().to_string(),
                Style::default().fg(item.kind.color()).bold(),
            ),
            Span::raw(" "),
            Span::raw(item.display_name_with_qty()),
        ]));
    }
    if game.inventory.is_empty() {
        lines.push(Line::from(tr("inventory.empty")));
    }
    lines
}

fn build_rename_lines(game: &Game, selected: usize, input: &str) -> Vec<Line<'static>> {
    let current = game
        .inventory_item_name(selected)
        .unwrap_or_else(|| tr("status.none").to_string());
    vec![
        Line::from(trf("rename.current", &[("name", current)])),
        Line::from(tr("rename.help")),
        Line::raw(""),
        Line::from(format!("> {}", input)),
    ]
}

fn render_crafting_modal(
    frame: &mut Frame,
    game: &Game,
    cursor: usize,
    selected_inv: usize,
    focus: CraftFocus,
    grid: &[Option<InventoryItem>; 9],
) {
    let area = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, area);
    let container = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_chrome_color(game)))
        .title(tr("title.crafting"));
    let inner = container.inner(area);
    frame.render_widget(container, area);

    let sub = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(inner);

    let instructions = Paragraph::new(vec![
        Line::from(tr("craft.help1")),
        Line::from(tr("craft.help2")),
    ]);
    frame.render_widget(instructions, sub[0]);

    let mut grid_lines: Vec<Line<'static>> = Vec::new();
    for row in 0..3 {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for col in 0..3 {
            let idx = row * 3 + col;
            let selected = idx == cursor;
            let (glyph, color) = match &grid[idx] {
                Some(item) => (item.kind.glyph(), item.kind.color()),
                None => ('·', Color::DarkGray),
            };
            let border_color = if selected {
                if focus == CraftFocus::Grid {
                    Color::Yellow
                } else {
                    Color::DarkGray
                }
            } else {
                Color::DarkGray
            };
            spans.push(Span::styled("[", Style::default().fg(border_color)));
            spans.push(Span::styled(
                glyph.to_string(),
                Style::default().fg(color).bold(),
            ));
            spans.push(Span::styled("]", Style::default().fg(border_color)));
            spans.push(Span::raw(" "));
        }
        grid_lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(grid_lines), sub[1]);

    frame.render_widget(Paragraph::new(Line::from(tr("craft.inventory"))), sub[2]);

    let mut inv_spans: Vec<Span<'static>> = Vec::new();
    if game.inventory.is_empty() {
        inv_spans.push(Span::raw(tr("inventory.empty")));
    } else {
        for (idx, item) in game.inventory.iter().enumerate() {
            let selected = idx == selected_inv;
            let border_color = if selected {
                if focus == CraftFocus::Inventory {
                    Color::Yellow
                } else {
                    Color::DarkGray
                }
            } else {
                Color::DarkGray
            };
            inv_spans.push(Span::styled("[", Style::default().fg(border_color)));
            inv_spans.push(Span::styled(
                item.kind.glyph().to_string(),
                Style::default().fg(item.kind.color()).bold(),
            ));
            inv_spans.push(Span::styled("]", Style::default().fg(border_color)));
            if item.qty > 1 {
                inv_spans.push(Span::raw(format!("x{}", item.qty)));
            }
            inv_spans.push(Span::raw(" "));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(inv_spans)), sub[3]);

    let selected_name = game
        .inventory
        .get(selected_inv)
        .map(InventoryItem::display_name)
        .unwrap_or_else(|| tr("status.none").to_string());
    let result_text = match find_recipe(grid) {
        Some(recipe) => trf("craft.result", &[("label", recipe.label.clone())]),
        None => tr("craft.no_result").to_string(),
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(trf("craft.selected", &[("name", selected_name)])),
            Line::from(result_text),
        ]),
        sub[4],
    );

    let craft_style = if focus == CraftFocus::CraftButton {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let craft_line = Line::from(vec![
        Span::raw("  "),
        Span::styled("[ ", craft_style),
        Span::styled(tr("craft.button"), craft_style),
        Span::styled(" ]", craft_style),
    ]);
    frame.render_widget(Paragraph::new(craft_line), sub[5]);
}

fn find_recipe(grid: &[Option<InventoryItem>; 9]) -> Option<&'static RecipeDef> {
    let kinds: [Option<Item>; 9] = std::array::from_fn(|i| grid[i].as_ref().map(|it| it.kind));
    for recipe in &defs().recipes {
        if kinds == recipe.inputs {
            return Some(recipe);
        }
    }
    None
}

fn fbm_noise01(
    perlin: &noise::Perlin2D,
    x: f64,
    y: f64,
    octaves: u32,
    persistence: f64,
    lacunarity: f64,
) -> f64 {
    let mut freq = 1.0;
    let mut amp = 1.0;
    let mut total = 0.0;
    let mut max_amp = 0.0;

    for _ in 0..octaves {
        total += perlin.noise01(x * freq, y * freq) * amp;
        max_amp += amp;
        amp *= persistence;
        freq *= lacunarity;
    }

    if max_amp == 0.0 { 0.0 } else { total / max_amp }
}

fn main() {
    text::init_from_env();
    let debug_enabled = env::args().any(|arg| arg == "--debug" || arg == "-d");
    let se_enabled = se_requested();
    if let Err(e) = run(debug_enabled, se_enabled) {
        eprintln!("{e}");
    }
}

fn se_requested() -> bool {
    if env::args().any(|arg| arg == "--no-se") {
        return false;
    }
    if env::args().any(|arg| arg == "--se") {
        return true;
    }
    match env::var("ABYSS_SE") {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "on" | "yes" => true,
            "0" | "false" | "off" | "no" => false,
            _ => true,
        },
        Err(_) => true,
    }
}

fn movement_delta(code: KeyCode) -> Option<(i32, i32)> {
    match code {
        KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
            'w' => Some((0, -1)),
            's' => Some((0, 1)),
            'a' => Some((-1, 0)),
            'd' => Some((1, 0)),
            'q' => Some((-1, -1)),
            'e' => Some((1, -1)),
            'z' => Some((-1, 1)),
            'c' => Some((1, 1)),
            _ => None,
        },
        KeyCode::Up => Some((0, -1)),
        KeyCode::Down => Some((0, 1)),
        KeyCode::Left => Some((-1, 0)),
        KeyCode::Right => Some((1, 0)),
        _ => None,
    }
}

fn has_adjacent_hostile_enemy(game: &Game) -> bool {
    game.enemies.iter().any(|e| {
        creature_meta(&e.creature_id).faction == crate::defs::Faction::Hostile
            && (e.pos.x - game.player.x)
                .abs()
                .max((e.pos.y - game.player.y).abs())
                == 1
    })
}

fn run_ctrl_auto_move_step(
    game: &mut Game,
    ui_mode: &mut UiMode,
    death_processed: &mut bool,
    sfx: &Option<SoundPlayer>,
    dx: i32,
    dy: i32,
    save_slot: usize,
) -> bool {
    if has_adjacent_hostile_enemy(game) {
        return false;
    }

    let next = (game.player.x + dx, game.player.y + dy);
    let stop_on_item = game.ground_items.contains_key(&next);
    if stop_on_item {
        game.suppress_auto_pickup_once();
    }
    let before = TurnSnapshot::capture(game);
    game.apply_action(Action::Move(dx, dy));
    if game.player_hp <= 0 {
        transition_to_dead(game, ui_mode, death_processed, save_slot);
        return false;
    }

    let moved = game.player.x != before.player_x || game.player.y != before.player_y;
    if moved && game.is_on_stairs() {
        *ui_mode = UiMode::StairsPrompt {
            selected_action: StairsAction::Descend,
        };
        if game.take_pending_vending() {
            *ui_mode = UiMode::Vending {
                cursor: 0,
                inserted_disks: 0,
            };
        }
        if let Some((title, text)) = game.take_pending_popup() {
            *ui_mode = UiMode::Dialogue { title, text };
        } else if let Some(text) = game.take_pending_dialogue() {
            game.push_log(text);
        }
        persist_game(game, save_slot);
        return false;
    }
    if game.take_pending_vending() {
        *ui_mode = UiMode::Vending {
            cursor: 0,
            inserted_disks: 0,
        };
    }
    if let Some((title, text)) = game.take_pending_popup() {
        *ui_mode = UiMode::Dialogue { title, text };
    } else if let Some(text) = game.take_pending_dialogue() {
        game.push_log(text);
    }
    play_turn_sfx(sfx, before, game, Some(SfxCue::Step));
    persist_game(game, save_slot);

    moved
        && matches!(ui_mode, UiMode::Normal)
        && !stop_on_item
        && !has_adjacent_hostile_enemy(game)
}

fn move_cursor_3x3(cursor: usize, dx: i32, dy: i32) -> usize {
    let x = (cursor % 3) as i32;
    let y = (cursor / 3) as i32;
    let nx = (x + dx).clamp(0, 2);
    let ny = (y + dy).clamp(0, 2);
    (ny * 3 + nx) as usize
}

fn close_crafting_mode(game: &mut Game, grid: &mut [Option<InventoryItem>; 9]) {
    for slot in grid.iter_mut() {
        if let Some(item) = slot.take() {
            game.stash_or_drop_item(item);
        }
    }
}

fn execute_crafting(game: &mut Game, grid: &mut [Option<InventoryItem>; 9]) -> bool {
    let Some(recipe) = find_recipe(grid) else {
        game.push_log_tr("craft.log.no_recipe");
        return false;
    };
    let result = recipe.result;

    for slot in grid.iter_mut() {
        *slot = None;
    }

    if game.add_item_kind_to_inventory(result) {
        game.push_log_trf("craft.log.crafted", &[("label", recipe.label.clone())]);
    } else if game.place_ground_item_near_player(result) {
        game.push_log_trf(
            "craft.log.crafted_drop",
            &[("label", recipe.label.clone())],
        );
    } else {
        game.push_log_trf(
            "craft.log.crafted_lost",
            &[("label", recipe.label.clone())],
        );
    }
    true
}

fn place_inventory_item_into_grid(
    game: &mut Game,
    grid: &mut [Option<InventoryItem>; 9],
    cursor: usize,
    selected_inv: &mut usize,
) {
    let inv_len = game.inventory_len();
    let selected = *selected_inv;
    if inv_len > 0 && selected < inv_len {
        let Some(picked) = game.take_inventory_one(selected) else {
            game.push_log_tr("craft.log.no_inv_selected");
            return;
        };
        let prev = grid[cursor].take();
        grid[cursor] = Some(picked);
        if let Some(prev_item) = prev {
            game.stash_or_drop_item(prev_item);
        }
        let new_len = game.inventory_len();
        if new_len == 0 {
            *selected_inv = 0;
        } else if *selected_inv >= new_len {
            *selected_inv = new_len - 1;
        }
    } else {
        game.push_log_tr("craft.log.no_inv_selected");
    }
}

fn load_game_from_slot(slot: usize) -> Result<Game, String> {
    let path = save_file_path(slot);
    if !path.exists() {
        return Err(tr("title_screen.load_missing").to_string());
    }
    match save::load_game(path.as_path()) {
        Ok(game) => Ok(game),
        Err(err) => Err(err),
    }
}

fn try_load_game(slot: usize) -> Result<Game, String> {
    let mut game = load_game_from_slot(slot)?;
    game.push_log_tr("save.loaded");
    Ok(game)
}

fn persist_game(game: &mut Game, save_slot: usize) {
    let path = save_file_path(save_slot);
    if let Err(err) = save::save_game(path.as_path(), game) {
        game.push_log_trf("save.failed", &[("error", err)]);
    }
}

fn erase_save_on_death(game: &mut Game, save_slot: usize) {
    let path = save_file_path(save_slot);
    if save::delete_save(path.as_path()).is_ok() {
        game.push_log_tr("save.deleted_on_death");
    } else {
        game.push_log_tr("save.delete_failed_on_death");
    }
}

fn delete_save_slot(slot: usize) -> Result<(), String> {
    let path = save_file_path(slot);
    save::delete_save(path.as_path())
}

fn transition_to_dead(
    game: &mut Game,
    ui_mode: &mut UiMode,
    death_processed: &mut bool,
    save_slot: usize,
) {
    if !*death_processed {
        erase_save_on_death(game, save_slot);
        *death_processed = true;
    }
    *ui_mode = UiMode::Dead {
        scroll: 0,
        selected_action: DeadAction::Restart,
    };
}

fn run(debug_enabled: bool, se_enabled: bool) -> io::Result<()> {
    let _guard = TerminalGuard::new()?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut game = Game::new(initial_seed());
    let mut save_slot: usize = 0;
    let mut esc_hold_count: u8 = 0;
    let mut ui_mode = UiMode::Title { selected: 0 };
    let mut death_processed = false;
    let mut ctrl_auto_move: Option<(i32, i32)> = None;
    let sfx = if se_enabled { SoundPlayer::new() } else { None };
    let mut observed_damage_taken = game.stat_damage_taken;

    loop {
        terminal.draw(|frame| render_ui(frame, &mut game, esc_hold_count, &ui_mode))?;
        game.advance_effects();
        if game.stat_damage_taken > observed_damage_taken {
            if let Some(sp) = sfx.as_ref() {
                sp.play(SfxCue::EnemyAttack);
            }
        }
        observed_damage_taken = game.stat_damage_taken;

        if game.player_hp <= 0 && !matches!(ui_mode, UiMode::Dead { .. }) {
            transition_to_dead(&mut game, &mut ui_mode, &mut death_processed, save_slot);
            continue;
        }

        if game.has_pending_effects() {
            while event::poll(Duration::from_millis(0))? {
                let _ = event::read()?;
            }
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        if let Some((dx, dy)) = ctrl_auto_move {
            if run_ctrl_auto_move_step(
                &mut game,
                &mut ui_mode,
                &mut death_processed,
                &sfx,
                dx,
                dy,
                save_slot,
            ) {
                std::thread::sleep(Duration::from_millis(35));
            } else {
                ctrl_auto_move = None;
            }
            continue;
        }

        if let UiMode::TitleTextTest { nonce, last_tick } = &mut ui_mode {
            let now = Instant::now();
            if now.duration_since(*last_tick) >= Duration::from_millis(25) {
                *nonce = nonce.wrapping_add(0x9E37_79B9_7F4A_7C15);
                *last_tick = now;
            }
        }

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        let ev = event::read()?;
        if let Event::Key(key) = ev {
            match &mut ui_mode {
                UiMode::Title { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc => break,
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *selected = selected.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *selected = (*selected + 1).min(TITLE_MENU_ENTRIES.len() - 1);
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            match TITLE_MENU_ENTRIES[*selected] {
                                TitleMenuEntry::Start => {
                                    if has_save_file() {
                                        ui_mode = UiMode::TitleSlotSelect {
                                            selected: 0,
                                            for_load: false,
                                        };
                                    } else {
                                        game = Game::new(initial_seed());
                                        save_slot = 0;
                                        persist_game(&mut game, save_slot);
                                        observed_damage_taken = game.stat_damage_taken;
                                        ui_mode = UiMode::Normal;
                                        esc_hold_count = 0;
                                        death_processed = false;
                                    }
                                }
                                TitleMenuEntry::Settings => {
                                    let langs = available_languages();
                                    let selected_lang = langs
                                        .iter()
                                        .position(|(code, _)| *code == current_lang())
                                        .unwrap_or(0);
                                    ui_mode = UiMode::Settings {
                                        selected: selected_lang,
                                        from_title: true,
                                    };
                                }
                                TitleMenuEntry::TextTest => {
                                    ui_mode = UiMode::TitleTextTest {
                                        nonce: initial_seed(),
                                        last_tick: Instant::now(),
                                    };
                                }
                                TitleMenuEntry::Exit => break,
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::TitleSlotSelect {
                    selected,
                    for_load: _,
                } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::Title { selected: 0 };
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *selected = selected.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *selected = (*selected + 1).min(SAVE_SLOT_COUNT - 1);
                        }
                        KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('x') | KeyCode::Char('X') => {
                            if has_save_in_slot(*selected) {
                                ui_mode = UiMode::TitleDeleteConfirm {
                                    selected: *selected,
                                };
                            }
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            if has_save_in_slot(*selected) {
                                match try_load_game(*selected) {
                                    Ok(loaded) => {
                                        game = loaded;
                                        save_slot = *selected;
                                        observed_damage_taken = game.stat_damage_taken;
                                        ui_mode = UiMode::Normal;
                                        esc_hold_count = 0;
                                        death_processed = false;
                                    }
                                    Err(err) => {
                                        game.push_log_trf("save.failed", &[("error", err)]);
                                    }
                                }
                            } else {
                                game = Game::new(initial_seed());
                                save_slot = *selected;
                                persist_game(&mut game, save_slot);
                                observed_damage_taken = game.stat_damage_taken;
                                ui_mode = UiMode::Normal;
                                esc_hold_count = 0;
                                death_processed = false;
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::TitleDeleteConfirm { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::TitleSlotSelect {
                                selected: *selected,
                                for_load: false,
                            };
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            match delete_save_slot(*selected) {
                                Ok(()) => {
                                    ui_mode = UiMode::TitleSlotSelect {
                                        selected: *selected,
                                        for_load: false,
                                    };
                                }
                                Err(err) => {
                                    game.push_log_trf("save.failed", &[("error", err)]);
                                    ui_mode = UiMode::TitleSlotSelect {
                                        selected: *selected,
                                        for_load: false,
                                    };
                                }
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::TitleTextTest { .. } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::Title { selected: 3 };
                        }
                        _ => {}
                    }
                }
                UiMode::Normal => {
                    if key.code == KeyCode::Esc {
                        if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                            esc_hold_count = esc_hold_count.saturating_add(1);
                            game.push_log_trf(
                                "log.hold_esc",
                                &[
                                    ("count", esc_hold_count.to_string()),
                                    ("max", ESC_HOLD_STEPS.to_string()),
                                ],
                            );
                            if esc_hold_count >= ESC_HOLD_STEPS {
                                break;
                            }
                        }
                        continue;
                    }

                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    esc_hold_count = 0;

                    let shift_only = key.modifiers.contains(KeyModifiers::SHIFT)
                        && !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT);

                    if debug_enabled && matches!(key.code, KeyCode::Char('/')) {
                        ui_mode = UiMode::DebugConsole {
                            input: String::new(),
                            suggestion_idx: 0,
                        };
                        continue;
                    }

                    if matches!(
                        key.code,
                        KeyCode::Char('v')
                            | KeyCode::Char('V')
                            | KeyCode::Char('m')
                            | KeyCode::Char('M')
                    ) {
                        ui_mode = UiMode::MainMenu { selected: 0 };
                        continue;
                    }

                    if let KeyCode::Char(ch) = key.code {
                        if ('1'..='9').contains(&ch) || ch == '0' {
                            let idx = if ch == '0' {
                                9
                            } else {
                                (ch as u8 - b'1') as usize
                            };
                            if idx < game.inventory_len() {
                                let before = TurnSnapshot::capture(&game);
                                if game.use_inventory_item(idx) {
                                    game.consume_non_attack_turn();
                                    play_turn_sfx(&sfx, before, &game, Some(SfxCue::Use));
                                    if game.player_hp <= 0 {
                                        transition_to_dead(
                                            &mut game,
                                            &mut ui_mode,
                                            &mut death_processed,
                                            save_slot,
                                        );
                                    } else {
                                        persist_game(&mut game, save_slot);
                                    }
                                }
                            }
                            continue;
                        }
                    }

                    let action = if let Some((dx, dy)) = movement_delta(key.code) {
                        let ctrl_move = key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT);
                        if ctrl_move {
                            ctrl_auto_move = Some((dx, dy));
                            continue;
                        }
                        if shift_only {
                            Some(Action::Face(dx, dy))
                        } else {
                            Some(Action::Move(dx, dy))
                        }
                    } else {
                        match key.code {
                            KeyCode::Char(' ') => Some(Action::Wait),
                            KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                                'f' => Some(Action::Attack),
                                _ => None,
                            },
                            _ => None,
                        }
                    };

                    if let Some(action) = action {
                        let before = TurnSnapshot::capture(&game);
                        let moved = matches!(action, Action::Move(_, _));
                        game.apply_action(action);
                        let fallback = match action {
                            Action::Move(_, _) => Some(SfxCue::Step),
                            Action::Attack => Some(SfxCue::AttackSwing),
                            Action::Wait | Action::Face(_, _) => None,
                        };
                        play_turn_sfx(&sfx, before, &game, fallback);
                        if game.player_hp <= 0 {
                            transition_to_dead(&mut game, &mut ui_mode, &mut death_processed, save_slot);
                        } else {
                            let stepped_on_stairs = moved
                                && (game.player.x != before.player_x || game.player.y != before.player_y)
                                && game.is_on_stairs();
                            if game.take_pending_vending() {
                                ui_mode = UiMode::Vending {
                                    cursor: 0,
                                    inserted_disks: 0,
                                };
                            } else if stepped_on_stairs {
                                ui_mode = UiMode::StairsPrompt {
                                    selected_action: StairsAction::Descend,
                                };
                            } else if let Some((title, text)) = game.take_pending_popup() {
                                ui_mode = UiMode::Dialogue { title, text };
                            } else if let Some(text) = game.take_pending_dialogue() {
                                game.push_log(text);
                            }
                            persist_game(&mut game, save_slot);
                        }
                    }
                }
                UiMode::DebugConsole {
                    input,
                    suggestion_idx,
                } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc => {
                            ui_mode = UiMode::Normal;
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            let len = debug_console_suggestions(input).len();
                            if len > 0 {
                                *suggestion_idx = suggestion_idx.saturating_sub(1);
                            }
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            let len = debug_console_suggestions(input).len();
                            if len > 0 {
                                *suggestion_idx = (*suggestion_idx + 1).min(len - 1);
                            }
                        }
                        KeyCode::Tab => {
                            if let Some(next) = apply_debug_suggestion(input, *suggestion_idx) {
                                *input = next;
                                *suggestion_idx = 0;
                            }
                        }
                        KeyCode::Enter => {
                            let cmd = input.trim().to_string();
                            if !cmd.is_empty() {
                                execute_debug_command(&mut game, &cmd);
                                persist_game(&mut game, save_slot);
                            }
                            ui_mode = UiMode::Normal;
                        }
                        KeyCode::Backspace => {
                            input.pop();
                            *suggestion_idx = 0;
                        }
                        KeyCode::Char(ch) => {
                            if !ch.is_control() && input.len() < 128 {
                                input.push(ch);
                                *suggestion_idx = 0;
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::StairsPrompt { selected_action } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Up
                        | KeyCode::Left
                        | KeyCode::Char('w')
                        | KeyCode::Char('W')
                        | KeyCode::Char('a')
                        | KeyCode::Char('A') => {
                            *selected_action = StairsAction::Descend;
                        }
                        KeyCode::Down
                        | KeyCode::Right
                        | KeyCode::Char('s')
                        | KeyCode::Char('S')
                        | KeyCode::Char('d')
                        | KeyCode::Char('D') => {
                            *selected_action = StairsAction::Stay;
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            if *selected_action == StairsAction::Descend {
                                game.descend_floor();
                                if let Some(sp) = sfx.as_ref() {
                                    sp.play(SfxCue::Stairs);
                                }
                                persist_game(&mut game, save_slot);
                            }
                            ui_mode = UiMode::Normal;
                        }
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::Normal;
                        }
                        _ => {}
                    }
                }
                UiMode::MainMenu { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::Normal
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *selected = selected.saturating_sub(1)
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *selected = (*selected + 1).min(MAIN_MENU_ENTRIES.len() - 1);
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            match MAIN_MENU_ENTRIES[*selected] {
                                MainMenuEntry::Items => {
                                    ui_mode = UiMode::Inventory {
                                        selected: 0,
                                        move_selected: false,
                                    }
                                }
                                MainMenuEntry::Crafting => {
                                    ui_mode = UiMode::Crafting {
                                        cursor: 0,
                                        selected_inv: 0,
                                        focus: CraftFocus::Grid,
                                        grid: std::array::from_fn(|_| None),
                                    };
                                }
                                MainMenuEntry::Settings => {
                                    let langs = available_languages();
                                    let selected = langs
                                        .iter()
                                        .position(|(code, _)| *code == current_lang())
                                        .unwrap_or(0);
                                    ui_mode = UiMode::Settings {
                                        selected,
                                        from_title: false,
                                    };
                                }
                                MainMenuEntry::Hints => ui_mode = UiMode::Hints,
                                MainMenuEntry::Feet => {
                                    if game.is_on_stairs() {
                                        ui_mode = UiMode::StairsPrompt {
                                            selected_action: StairsAction::Descend,
                                        };
                                    } else if game.ground_item_at_player().is_some() {
                                        ui_mode = UiMode::GroundItemMenu { action_idx: 0 };
                                    } else {
                                        ui_mode = UiMode::Normal;
                                    }
                                }
                                MainMenuEntry::Title => {
                                    persist_game(&mut game, save_slot);
                                    ui_mode = UiMode::Title { selected: 0 };
                                }
                                MainMenuEntry::Exit => break,
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::Settings {
                    selected,
                    from_title,
                } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let langs = available_languages();
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = if *from_title {
                                UiMode::Title { selected: 2 }
                            } else {
                                UiMode::MainMenu { selected: 0 }
                            }
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *selected = selected.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            if !langs.is_empty() {
                                *selected = (*selected + 1).min(langs.len() - 1);
                            }
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            if let Some((code, _name)) = langs.get(*selected) {
                                if set_lang(code) {
                                    if !*from_title {
                                        game.push_log_trf(
                                            "settings.lang_changed",
                                            &[("name", log_arg_text_ref(&format!("lang.name.{code}")))],
                                        );
                                        persist_game(&mut game, save_slot);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::Hints => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if matches!(
                        key.code,
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V')
                    ) {
                        ui_mode = UiMode::MainMenu { selected: 0 };
                    }
                }
                UiMode::Dialogue { .. } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if matches!(
                        key.code,
                        KeyCode::Esc
                            | KeyCode::Enter
                            | KeyCode::Char('f')
                            | KeyCode::Char('F')
                            | KeyCode::Char('v')
                            | KeyCode::Char('V')
                    ) {
                        ui_mode = UiMode::Normal;
                    }
                }
                UiMode::Vending {
                    cursor,
                    inserted_disks,
                } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            if *inserted_disks > 0 {
                                game.player_copper_disks =
                                    game.player_copper_disks.saturating_add(*inserted_disks);
                                game.push_log_tr("vending.log.refund");
                            }
                            persist_game(&mut game, save_slot);
                            ui_mode = UiMode::Normal;
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *cursor = move_vending_cursor(*cursor, 0, -1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *cursor = move_vending_cursor(*cursor, 0, 1);
                        }
                        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
                            *cursor = move_vending_cursor(*cursor, -1, 0);
                        }
                        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                            *cursor = move_vending_cursor(*cursor, 1, 0);
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            if *cursor == 12 {
                                if game.player_copper_disks > 0 {
                                    game.player_copper_disks =
                                        game.player_copper_disks.saturating_sub(1);
                                    *inserted_disks = (*inserted_disks).saturating_add(1);
                                    game.push_log_tr("vending.log.insert");
                                } else {
                                    game.push_log_tr("vending.log.no_copper");
                                }
                                persist_game(&mut game, save_slot);
                                continue;
                            }
                            let Some(product) = vending_selected_product(*cursor) else {
                                continue;
                            };
                            if *inserted_disks < product.price_as {
                                game.push_log_tr("vending.log.not_enough");
                                continue;
                            }
                            if game.add_item_kind_to_inventory(product.item) {
                                *inserted_disks -= product.price_as;
                                game.push_log_trf(
                                    "vending.log.bought",
                                    &[("item", log_arg_item_ref(product.item))],
                                );
                                persist_game(&mut game, save_slot);
                            } else {
                                game.push_log_tr("vending.log.inventory_full");
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::Inventory {
                    selected,
                    move_selected,
                } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let len = game.inventory_len();
                    let is_shift_f = matches!(key.code, KeyCode::Char('F'))
                        || (matches!(key.code, KeyCode::Char('f'))
                            && key.modifiers.contains(KeyModifiers::SHIFT));
                    if len == 0 {
                        *move_selected = false;
                    }
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            *move_selected = false;
                            ui_mode = UiMode::MainMenu { selected: 0 }
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            if len > 0 {
                                if *move_selected {
                                    if *selected > 0
                                        && game.move_inventory_item(*selected, *selected - 1)
                                    {
                                        *selected = selected.saturating_sub(1);
                                        persist_game(&mut game, save_slot);
                                    }
                                } else {
                                    *selected = selected.saturating_sub(1);
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            if len > 0 {
                                if *move_selected {
                                    if *selected + 1 < len
                                        && game.move_inventory_item(*selected, *selected + 1)
                                    {
                                        *selected = (*selected + 1).min(len - 1);
                                        persist_game(&mut game, save_slot);
                                    }
                                } else {
                                    *selected = (*selected + 1).min(len - 1);
                                }
                            }
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            if len > 0 {
                                if is_shift_f {
                                    *move_selected = !*move_selected;
                                    continue;
                                }
                                if *move_selected {
                                    continue;
                                }
                                *selected = (*selected).min(len - 1);
                                ui_mode = UiMode::ItemMenu {
                                    selected: *selected,
                                    action_idx: 0,
                                };
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::ItemMenu {
                    selected,
                    action_idx,
                } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let len = game.inventory_len();
                    if len == 0 {
                        ui_mode = UiMode::Inventory {
                            selected: 0,
                            move_selected: false,
                        };
                        continue;
                    }
                    *selected = (*selected).min(len - 1);
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::Inventory {
                                selected: *selected,
                                move_selected: false,
                            };
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *action_idx = action_idx.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *action_idx = (*action_idx + 1).min(ITEM_MENU_ACTIONS.len() - 1);
                        }
                        KeyCode::Enter | KeyCode::Char('f') => {
                            let item_idx = *selected;
                            match ITEM_MENU_ACTIONS[*action_idx] {
                                ItemMenuAction::Rename => {
                                    let current =
                                        game.inventory_item_name(item_idx).unwrap_or_default();
                                    ui_mode = UiMode::RenameItem {
                                        selected: item_idx,
                                        input: current,
                                    };
                                }
                                ItemMenuAction::Drop => {
                                    let before = TurnSnapshot::capture(&game);
                                    if game.drop_inventory_item(item_idx) {
                                        game.consume_non_attack_turn();
                                        play_turn_sfx(&sfx, before, &game, Some(SfxCue::Drop));
                                        if game.player_hp <= 0 {
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                                save_slot,
                                            );
                                        } else {
                                            persist_game(&mut game, save_slot);
                                            ui_mode = UiMode::Normal;
                                        }
                                    } else {
                                        let next_len = game.inventory_len();
                                        ui_mode = UiMode::ItemMenu {
                                            selected: item_idx.min(next_len.saturating_sub(1)),
                                            action_idx: *action_idx,
                                        };
                                    }
                                }
                                ItemMenuAction::Throw => {
                                    let before = TurnSnapshot::capture(&game);
                                    if game.throw_inventory_item(item_idx) {
                                        game.consume_non_attack_turn();
                                        play_turn_sfx(&sfx, before, &game, Some(SfxCue::Throw));
                                        if game.player_hp <= 0 {
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                                save_slot,
                                            );
                                        } else {
                                            persist_game(&mut game, save_slot);
                                            ui_mode = UiMode::Normal;
                                        }
                                    } else {
                                        let next_len = game.inventory_len();
                                        ui_mode = UiMode::ItemMenu {
                                            selected: item_idx.min(next_len.saturating_sub(1)),
                                            action_idx: *action_idx,
                                        };
                                    }
                                }
                                ItemMenuAction::Use => {
                                    let before = TurnSnapshot::capture(&game);
                                    if game.use_inventory_item(item_idx) {
                                        game.consume_non_attack_turn();
                                        play_turn_sfx(&sfx, before, &game, Some(SfxCue::Use));
                                        if game.player_hp <= 0 {
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                                save_slot,
                                            );
                                        } else {
                                            persist_game(&mut game, save_slot);
                                            ui_mode = UiMode::Normal;
                                        }
                                    } else {
                                        let next_len = game.inventory_len();
                                        ui_mode = UiMode::ItemMenu {
                                            selected: item_idx.min(next_len.saturating_sub(1)),
                                            action_idx: *action_idx,
                                        };
                                    }
                                }
                            }
                            if game.player_hp <= 0 {
                                transition_to_dead(&mut game, &mut ui_mode, &mut death_processed, save_slot);
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::GroundItemMenu { action_idx } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if game.ground_item_at_player().is_none() {
                        ui_mode = UiMode::Normal;
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::MainMenu { selected: 0 };
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *action_idx = action_idx.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *action_idx = (*action_idx + 1).min(GROUND_ITEM_MENU_ACTIONS.len() - 1);
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            match GROUND_ITEM_MENU_ACTIONS[*action_idx] {
                                GroundItemMenuAction::Pick => {
                                    let before = TurnSnapshot::capture(&game);
                                    if game.pick_up_item_at_player() {
                                        game.consume_non_attack_turn();
                                        play_turn_sfx(&sfx, before, &game, Some(SfxCue::Pickup));
                                        if game.player_hp <= 0 {
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                                save_slot,
                                            );
                                        } else {
                                            persist_game(&mut game, save_slot);
                                            ui_mode = UiMode::Normal;
                                        }
                                    } else {
                                        ui_mode = UiMode::Normal;
                                    }
                                }
                                GroundItemMenuAction::Swap => {
                                    if game.inventory_len() == 0 {
                                        game.push_log_tr("inventory.empty");
                                        ui_mode = UiMode::GroundItemMenu { action_idx: 0 };
                                    } else {
                                        ui_mode = UiMode::GroundSwapSelect { selected: 0 };
                                    }
                                }
                                GroundItemMenuAction::Use => {
                                    let Some(kind) = game.pick_up_item_at_player_kind() else {
                                        ui_mode = UiMode::Normal;
                                        continue;
                                    };
                                    let Some(idx) = game.first_inventory_index_of_kind(kind) else {
                                        ui_mode = UiMode::Normal;
                                        continue;
                                    };
                                    let before = TurnSnapshot::capture(&game);
                                    if game.use_inventory_item(idx) {
                                        game.consume_non_attack_turn();
                                        play_turn_sfx(&sfx, before, &game, Some(SfxCue::Use));
                                        if game.player_hp <= 0 {
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                                save_slot,
                                            );
                                        } else {
                                            persist_game(&mut game, save_slot);
                                            ui_mode = UiMode::Normal;
                                        }
                                    } else {
                                        ui_mode = UiMode::Normal;
                                    }
                                }
                                GroundItemMenuAction::Throw => {
                                    let Some(kind) = game.pick_up_item_at_player_kind() else {
                                        ui_mode = UiMode::Normal;
                                        continue;
                                    };
                                    let Some(idx) = game.first_inventory_index_of_kind(kind) else {
                                        ui_mode = UiMode::Normal;
                                        continue;
                                    };
                                    let before = TurnSnapshot::capture(&game);
                                    if game.throw_inventory_item(idx) {
                                        game.consume_non_attack_turn();
                                        play_turn_sfx(&sfx, before, &game, Some(SfxCue::Throw));
                                        if game.player_hp <= 0 {
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                                save_slot,
                                            );
                                        } else {
                                            persist_game(&mut game, save_slot);
                                            ui_mode = UiMode::Normal;
                                        }
                                    } else {
                                        ui_mode = UiMode::Normal;
                                    }
                                }
                                GroundItemMenuAction::Rename => {
                                    let Some(kind) = game.pick_up_item_at_player_kind() else {
                                        ui_mode = UiMode::Normal;
                                        continue;
                                    };
                                    let Some(idx) = game.first_inventory_index_of_kind(kind) else {
                                        ui_mode = UiMode::Normal;
                                        continue;
                                    };
                                    let current = game.inventory_item_name(idx).unwrap_or_default();
                                    persist_game(&mut game, save_slot);
                                    ui_mode = UiMode::RenameItem {
                                        selected: idx,
                                        input: current,
                                    };
                                }
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::GroundSwapSelect { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let len = game.inventory_len();
                    if game.ground_item_at_player().is_none() {
                        ui_mode = UiMode::Normal;
                        continue;
                    }
                    if len == 0 {
                        ui_mode = UiMode::GroundItemMenu { action_idx: 0 };
                        continue;
                    }
                    *selected = (*selected).min(len - 1);
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::GroundItemMenu { action_idx: 0 };
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *selected = selected.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *selected = (*selected + 1).min(len - 1);
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            let before = TurnSnapshot::capture(&game);
                            if game.swap_ground_item_with_inventory(*selected) {
                                game.consume_non_attack_turn();
                                play_turn_sfx(&sfx, before, &game, Some(SfxCue::Pickup));
                                if game.player_hp <= 0 {
                                    transition_to_dead(
                                        &mut game,
                                        &mut ui_mode,
                                        &mut death_processed,
                                        save_slot,
                                    );
                                } else {
                                    persist_game(&mut game, save_slot);
                                    ui_mode = UiMode::Normal;
                                }
                            } else {
                                ui_mode = UiMode::GroundItemMenu { action_idx: 0 };
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::RenameItem { selected, input } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let len = game.inventory_len();
                    if len == 0 {
                        ui_mode = UiMode::Inventory {
                            selected: 0,
                            move_selected: false,
                        };
                        continue;
                    }
                    let item_idx = (*selected).min(len - 1);
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::ItemMenu {
                                selected: item_idx,
                                action_idx: 0,
                            };
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            let name = input.clone();
                            let _ = game.rename_inventory_item(item_idx, name);
                            persist_game(&mut game, save_slot);
                            ui_mode = UiMode::ItemMenu {
                                selected: item_idx,
                                action_idx: 0,
                            };
                        }
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        KeyCode::Char(ch) => {
                            if !ch.is_control() && input.len() < 24 {
                                input.push(ch);
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::Crafting {
                    cursor,
                    selected_inv,
                    focus,
                    grid,
                } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let inv_len = game.inventory_len();
                    if inv_len == 0 {
                        *selected_inv = 0;
                    } else if *selected_inv >= inv_len {
                        *selected_inv = inv_len - 1;
                    }

                    match key.code {
                        KeyCode::Esc | KeyCode::Tab | KeyCode::Char('v') | KeyCode::Char('V') => {
                            if *focus == CraftFocus::Inventory {
                                *focus = CraftFocus::Grid;
                            } else {
                                close_crafting_mode(&mut game, grid);
                                ui_mode = UiMode::MainMenu { selected: 1 };
                            }
                        }
                        KeyCode::Up => match *focus {
                            CraftFocus::Grid => {
                                *cursor = move_cursor_3x3(*cursor, 0, -1);
                            }
                            CraftFocus::Inventory | CraftFocus::CraftButton => {
                                *focus = CraftFocus::Grid;
                            }
                        },
                        KeyCode::Down => match *focus {
                            CraftFocus::Grid => {
                                if (*cursor / 3) >= 2 {
                                    *focus = CraftFocus::CraftButton;
                                } else {
                                    *cursor = move_cursor_3x3(*cursor, 0, 1);
                                }
                            }
                            CraftFocus::Inventory => {
                                *focus = CraftFocus::CraftButton;
                            }
                            CraftFocus::CraftButton => {}
                        },
                        KeyCode::Left => match *focus {
                            CraftFocus::Grid => {
                                *cursor = move_cursor_3x3(*cursor, -1, 0);
                            }
                            CraftFocus::Inventory => {
                                if inv_len > 0 {
                                    *selected_inv = selected_inv.saturating_sub(1);
                                }
                            }
                            CraftFocus::CraftButton => {
                                if inv_len > 0 {
                                    *focus = CraftFocus::Inventory;
                                }
                            }
                        },
                        KeyCode::Right => match *focus {
                            CraftFocus::Grid => {
                                *cursor = move_cursor_3x3(*cursor, 1, 0);
                            }
                            CraftFocus::Inventory => {
                                if inv_len > 0 {
                                    *selected_inv = (*selected_inv + 1).min(inv_len - 1);
                                }
                            }
                            CraftFocus::CraftButton => {
                                if inv_len > 0 {
                                    *focus = CraftFocus::Inventory;
                                }
                            }
                        },
                        KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                            'w' => match *focus {
                                CraftFocus::Grid => {
                                    *cursor = move_cursor_3x3(*cursor, 0, -1);
                                }
                                CraftFocus::Inventory | CraftFocus::CraftButton => {
                                    *focus = CraftFocus::Grid;
                                }
                            },
                            's' => match *focus {
                                CraftFocus::Grid => {
                                    if (*cursor / 3) >= 2 {
                                        *focus = CraftFocus::CraftButton;
                                    } else {
                                        *cursor = move_cursor_3x3(*cursor, 0, 1);
                                    }
                                }
                                CraftFocus::Inventory => {
                                    *focus = CraftFocus::CraftButton;
                                }
                                CraftFocus::CraftButton => {}
                            },
                            'a' => match *focus {
                                CraftFocus::Grid => {
                                    *cursor = move_cursor_3x3(*cursor, -1, 0);
                                }
                                CraftFocus::Inventory => {
                                    if inv_len > 0 {
                                        *selected_inv = selected_inv.saturating_sub(1);
                                    }
                                }
                                CraftFocus::CraftButton => {
                                    if inv_len > 0 {
                                        *focus = CraftFocus::Inventory;
                                    }
                                }
                            },
                            'd' => match *focus {
                                CraftFocus::Grid => {
                                    *cursor = move_cursor_3x3(*cursor, 1, 0);
                                }
                                CraftFocus::Inventory => {
                                    if inv_len > 0 {
                                        *selected_inv = (*selected_inv + 1).min(inv_len - 1);
                                    }
                                }
                                CraftFocus::CraftButton => {
                                    if inv_len > 0 {
                                        *focus = CraftFocus::Inventory;
                                    }
                                }
                            },
                            'f' | ' ' => match *focus {
                                CraftFocus::Grid => {
                                    let idx = *cursor;
                                    if let Some(item) = grid[idx].take() {
                                        game.stash_or_drop_item(item);
                                    } else if inv_len > 0 {
                                        *focus = CraftFocus::Inventory;
                                    } else {
                                        game.push_log_tr("log.no_inv_to_place");
                                    }
                                }
                                CraftFocus::Inventory => {
                                    place_inventory_item_into_grid(
                                        &mut game,
                                        grid,
                                        *cursor,
                                        selected_inv,
                                    );
                                    *focus = CraftFocus::Grid;
                                }
                                CraftFocus::CraftButton => {
                                    let before = TurnSnapshot::capture(&game);
                                    if execute_crafting(&mut game, grid) {
                                        game.consume_non_attack_turn();
                                        play_turn_sfx(&sfx, before, &game, Some(SfxCue::Craft));
                                        if game.player_hp <= 0 {
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                                save_slot,
                                            );
                                        } else {
                                            persist_game(&mut game, save_slot);
                                        }
                                    }
                                }
                            },
                            _ => {}
                        },
                        KeyCode::Enter => match *focus {
                            CraftFocus::Grid => {
                                let idx = *cursor;
                                if let Some(item) = grid[idx].take() {
                                    game.stash_or_drop_item(item);
                                } else if inv_len > 0 {
                                    *focus = CraftFocus::Inventory;
                                } else {
                                    game.push_log_tr("log.no_inv_to_place");
                                }
                            }
                            CraftFocus::Inventory => {
                                place_inventory_item_into_grid(
                                    &mut game,
                                    grid,
                                    *cursor,
                                    selected_inv,
                                );
                                *focus = CraftFocus::Grid;
                            }
                            CraftFocus::CraftButton => {
                                let before = TurnSnapshot::capture(&game);
                                if execute_crafting(&mut game, grid) {
                                    game.consume_non_attack_turn();
                                    play_turn_sfx(&sfx, before, &game, Some(SfxCue::Craft));
                                    if game.player_hp <= 0 {
                                        transition_to_dead(
                                            &mut game,
                                            &mut ui_mode,
                                            &mut death_processed,
                                            save_slot,
                                        );
                                    } else {
                                        persist_game(&mut game, save_slot);
                                    }
                                }
                            }
                        },
                        KeyCode::Backspace | KeyCode::Delete => {
                            if *focus == CraftFocus::Grid {
                                let idx = *cursor;
                                if let Some(item) = grid[idx].take() {
                                    game.stash_or_drop_item(item);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                UiMode::Dead {
                    scroll,
                    selected_action,
                } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *scroll = scroll.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *scroll = scroll.saturating_add(1);
                        }
                        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
                            *selected_action = DeadAction::Restart;
                        }
                        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                            *selected_action = DeadAction::Exit;
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                            match *selected_action {
                                DeadAction::Restart => {
                                    game = Game::new(initial_seed());
                                    observed_damage_taken = game.stat_damage_taken;
                                    ui_mode = UiMode::Normal;
                                    esc_hold_count = 0;
                                    death_processed = false;
                                }
                                DeadAction::Exit => break,
                            }
                        }
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => break,
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

fn initial_seed() -> u64 {
    if let Ok(raw) = env::var("ABYSS_SEED") {
        if let Ok(seed) = raw.parse::<u64>() {
            return seed;
        }
    }
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as u64,
        Err(_) => 0xC0FFEE_u64 ^ 0xA11CE_u64,
    }
}
