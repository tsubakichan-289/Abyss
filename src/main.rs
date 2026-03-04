mod noise;

use std::collections::HashMap;
use std::io;
use std::sync::OnceLock;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use serde::Deserialize;

const CHUNK_SIZE: usize = 16;
const CHUNK_AREA: usize = CHUNK_SIZE * CHUNK_SIZE;
const VISION_RADIUS: i32 = 7;
const ABYSS_THRESHOLD: f64 = 0.22;
const ROCK_THRESHOLD: f64 = 0.63;
const WALL_THRESHOLD: f64 = 0.80;
const ESC_HOLD_STEPS: u8 = 8;
const POTION_HEAL: i32 = 6;
const MAX_INVENTORY: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Potion,
    Wood,
    Stone,
    StringFiber,
    StoneAxe,
}

impl Item {
    fn key(self) -> &'static str {
        match self {
            Self::Potion => "potion",
            Self::Wood => "wood",
            Self::Stone => "stone",
            Self::StringFiber => "string",
            Self::StoneAxe => "stone_axe",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "potion" => Some(Self::Potion),
            "wood" => Some(Self::Wood),
            "stone" => Some(Self::Stone),
            "string" => Some(Self::StringFiber),
            "stone_axe" => Some(Self::StoneAxe),
            _ => None,
        }
    }

    fn glyph(self) -> char {
        item_meta(self).glyph
    }

    fn color(self) -> Color {
        item_meta(self).color
    }

    fn name(self) -> &'static str {
        &item_meta(self).name
    }
}

#[derive(Clone, Debug)]
struct InventoryItem {
    kind: Item,
    custom_name: Option<String>,
}

impl InventoryItem {
    fn display_name(&self) -> String {
        match &self.custom_name {
            Some(name) if !name.is_empty() => name.clone(),
            _ => self.kind.name().to_string(),
        }
    }
}

impl Tile {
    fn key(self) -> &'static str {
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
        }
    }

    fn from_key(key: &str) -> Option<Self> {
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

#[derive(Deserialize)]
struct GameDataFile {
    tiles: Vec<TileDefRaw>,
    items: Vec<ItemDefRaw>,
    recipes: Vec<RecipeDefRaw>,
}

#[derive(Deserialize)]
struct TileDefRaw {
    id: String,
    glyph: String,
    color: u8,
    shadow_color: Option<u8>,
    walkable: bool,
    legend: String,
    harvest_hits: Option<u8>,
    harvest_drop: Option<String>,
    harvest_drop_chance: Option<u8>,
    harvest_replace: Option<String>,
    harvest_label: Option<String>,
}

#[derive(Deserialize)]
struct ItemDefRaw {
    id: String,
    name: String,
    glyph: String,
    color: u8,
    legend: String,
}

#[derive(Deserialize)]
struct RecipeDefRaw {
    result: String,
    label: Option<String>,
    inputs: Vec<String>,
}

#[derive(Clone)]
struct TileDef {
    glyph: char,
    color: Color,
    shadow_color: Color,
    walkable: bool,
    legend: String,
    harvest_hits: Option<u8>,
    harvest_drop: Option<Item>,
    harvest_drop_chance: u8,
    harvest_replace: Option<Tile>,
    harvest_label: Option<String>,
}

#[derive(Clone)]
struct ItemDef {
    name: String,
    glyph: char,
    color: Color,
    legend: String,
}

#[derive(Clone)]
struct RecipeDef {
    result: Item,
    label: String,
    inputs: [Option<Item>; 9],
}

struct GameDefs {
    tiles: HashMap<String, TileDef>,
    items: HashMap<String, ItemDef>,
    recipes: Vec<RecipeDef>,
}

fn defs() -> &'static GameDefs {
    static DEFS: OnceLock<GameDefs> = OnceLock::new();
    DEFS.get_or_init(|| load_defs())
}

fn parse_single_char(s: &str, kind: &str, id: &str) -> char {
    let mut it = s.chars();
    let c = it
        .next()
        .unwrap_or_else(|| panic!("{kind} '{id}' has empty glyph"));
    assert!(
        it.next().is_none(),
        "{kind} '{id}' glyph must be exactly 1 char"
    );
    c
}

fn load_defs() -> GameDefs {
    let raw = include_str!("../data/game_data.toml");
    let parsed: GameDataFile =
        toml::from_str(raw).expect("failed to parse data/game_data.toml");

    let mut items = HashMap::new();
    for it in parsed.items {
        let item = Item::from_key(&it.id)
            .unwrap_or_else(|| panic!("unknown item id in data file: {}", it.id));
        let _ = item;
        items.insert(
            it.id.clone(),
            ItemDef {
                name: it.name,
                glyph: parse_single_char(&it.glyph, "item", &it.id),
                color: Color::Indexed(it.color),
                legend: it.legend,
            },
        );
    }

    let mut tiles = HashMap::new();
    for t in parsed.tiles {
        let tile = Tile::from_key(&t.id)
            .unwrap_or_else(|| panic!("unknown tile id in data file: {}", t.id));
        let _ = tile;
        let harvest_drop = t
            .harvest_drop
            .as_deref()
            .map(|k| Item::from_key(k).unwrap_or_else(|| panic!("unknown harvest_drop item: {k}")));
        let harvest_replace = t.harvest_replace.as_deref().map(|k| {
            Tile::from_key(k).unwrap_or_else(|| panic!("unknown harvest_replace tile: {k}"))
        });
        tiles.insert(
            t.id.clone(),
            TileDef {
                glyph: parse_single_char(&t.glyph, "tile", &t.id),
                color: Color::Indexed(t.color),
                shadow_color: Color::Indexed(t.shadow_color.unwrap_or(t.color)),
                walkable: t.walkable,
                legend: t.legend,
                harvest_hits: t.harvest_hits,
                harvest_drop,
                harvest_drop_chance: t.harvest_drop_chance.unwrap_or(100),
                harvest_replace,
                harvest_label: t.harvest_label,
            },
        );
    }

    let mut recipes = Vec::new();
    for r in parsed.recipes {
        assert!(r.inputs.len() == 9, "recipe '{}' must have 9 inputs", r.result);
        let mut inputs: [Option<Item>; 9] = [None; 9];
        for (i, k) in r.inputs.iter().enumerate() {
            let trimmed = k.trim();
            if trimmed.is_empty() {
                inputs[i] = None;
            } else {
                inputs[i] = Some(
                    Item::from_key(trimmed)
                        .unwrap_or_else(|| panic!("unknown recipe input item: {}", trimmed)),
                );
            }
        }
        let result = Item::from_key(&r.result)
            .unwrap_or_else(|| panic!("unknown recipe result item: {}", r.result));
        recipes.push(RecipeDef {
            result,
            label: r.label.unwrap_or_else(|| r.result.clone()),
            inputs,
        });
    }

    GameDefs {
        tiles,
        items,
        recipes,
    }
}

fn tile_meta(tile: Tile) -> &'static TileDef {
    let key = tile.key();
    defs()
        .tiles
        .get(key)
        .unwrap_or_else(|| panic!("tile '{}' missing in data file", key))
}

fn item_meta(item: Item) -> &'static ItemDef {
    let key = item.key();
    defs()
        .items
        .get(key)
        .unwrap_or_else(|| panic!("item '{}' missing in data file", key))
}

#[derive(Clone, Copy, Debug)]
struct Pos {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            Self::N => "N",
            Self::NE => "NE",
            Self::E => "E",
            Self::SE => "SE",
            Self::S => "S",
            Self::SW => "SW",
            Self::W => "W",
            Self::NW => "NW",
        }
    }

    fn glyph(self) -> char {
        match self {
            Self::N => '^',
            Self::NE => '/',
            Self::E => '>',
            Self::SE => '\\',
            Self::S => 'v',
            Self::SW => '/',
            Self::W => '<',
            Self::NW => '\\',
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

#[derive(Clone, Copy, Debug)]
struct Enemy {
    pos: Pos,
    hp: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Move(i32, i32),
    Face(i32, i32),
    Attack,
    Wait,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveResult {
    Moved,
    Blocked,
    RotatedOnly,
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
            Self::Rename => "Rename",
            Self::Drop => "Drop",
            Self::Throw => "Throw",
            Self::Use => "Use",
        }
    }
}

const ITEM_MENU_ACTIONS: [ItemMenuAction; 4] = [
    ItemMenuAction::Rename,
    ItemMenuAction::Drop,
    ItemMenuAction::Throw,
    ItemMenuAction::Use,
];

#[derive(Clone, Debug)]
enum UiMode {
    Normal,
    Inventory { selected: usize },
    ItemMenu { selected: usize, action_idx: usize },
    RenameItem { selected: usize, input: String },
    Crafting {
        cursor: usize,
        selected_inv: usize,
        focus: CraftFocus,
        grid: [Option<InventoryItem>; 9],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CraftFocus {
    Grid,
    Inventory,
}

#[derive(Clone, Copy, Debug)]
struct HarvestState {
    target: (i32, i32),
    hits: u8,
}

#[derive(Clone)]
struct Chunk {
    tiles: [Tile; CHUNK_AREA],
}

impl Chunk {
    fn new(fill: Tile) -> Self {
        Self {
            tiles: [fill; CHUNK_AREA],
        }
    }

    fn idx(local_x: usize, local_y: usize) -> usize {
        local_y * CHUNK_SIZE + local_x
    }

    fn get(&self, local_x: usize, local_y: usize) -> Tile {
        self.tiles[Self::idx(local_x, local_y)]
    }

    fn set(&mut self, local_x: usize, local_y: usize, tile: Tile) {
        let idx = Self::idx(local_x, local_y);
        self.tiles[idx] = tile;
    }
}

struct World {
    seed: u64,
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
        v.div_euclid(CHUNK_SIZE as i32)
    }

    fn local_coord(v: i32) -> usize {
        v.rem_euclid(CHUNK_SIZE as i32) as usize
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

    fn generate_chunk(seed: u64, chunk_x: i32, chunk_y: i32) -> Chunk {
        let mut chunk = Chunk::new(Tile::DeepWater);
        let terrain_noise = noise::Perlin2D::new(seed);
        let scale = 0.05;
        let octaves = 4;
        let persistence = 0.5;
        let lacunarity = 2.0;

        let base_x = chunk_x * CHUNK_SIZE as i32;
        let base_y = chunk_y * CHUNK_SIZE as i32;

        for local_y in 0..CHUNK_SIZE {
            for local_x in 0..CHUNK_SIZE {
                let world_x = base_x + local_x as i32;
                let world_y = base_y + local_y as i32;
                let h = fbm_noise01(
                    &terrain_noise,
                    world_x as f64 * scale,
                    world_y as f64 * scale,
                    octaves,
                    persistence,
                    lacunarity,
                );
                let tile = if h <= ABYSS_THRESHOLD {
                    Tile::Abyss
                } else if h >= WALL_THRESHOLD {
                    Tile::Wall
                } else if h >= ROCK_THRESHOLD {
                    Tile::Rock
                } else {
                    Tile::from_height(h)
                };

                chunk.set(local_x, local_y, tile);
            }
        }

        chunk
    }
}

struct Game {
    world: World,
    player: Pos,
    facing: Facing,
    player_hp: i32,
    player_max_hp: i32,
    inventory: Vec<InventoryItem>,
    equipped_tool: Option<InventoryItem>,
    enemies: Vec<Enemy>,
    ground_items: HashMap<(i32, i32), Item>,
    harvest_state: Option<HarvestState>,
    rng_state: u64,
    turn: u64,
    logs: Vec<String>,
}

impl Game {
    fn new(seed: u64) -> Self {
        let mut game = Self {
            world: World::new(seed),
            player: Pos { x: 0, y: 0 },
            facing: Facing::S,
            player_hp: 20,
            player_max_hp: 20,
            inventory: Vec::new(),
            equipped_tool: None,
            enemies: Vec::new(),
            ground_items: HashMap::new(),
            harvest_state: None,
            rng_state: seed ^ 0xA5A5_5A5A_DEAD_BEEF,
            turn: 0,
            logs: vec![String::from("Use WASD to move, F to attack")],
        };
        game.player = game.find_spawn();
        game.spawn_enemies(12);
        game.spawn_items(10);
        game
    }

    fn tile(&mut self, x: i32, y: i32) -> Tile {
        self.world.tile(x, y)
    }

    fn set_tile(&mut self, x: i32, y: i32, tile: Tile) {
        self.world.set_tile(x, y, tile);
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
        if self.has_enemy_at(nx, ny) {
            if self.facing != old_facing {
                self.push_log(format!("Facing {}", self.facing.label()));
                return MoveResult::RotatedOnly;
            }
            self.push_log("An enemy blocks the way");
            return MoveResult::Blocked;
        }
        if self.tile(nx, ny).walkable() {
            self.player = Pos { x: nx, y: ny };
            self.push_log(format!("Moved to ({nx}, {ny})"));
            self.pick_up_item_at_player();
            MoveResult::Moved
        } else {
            if self.facing != old_facing {
                self.push_log(format!("Facing {}", self.facing.label()));
                return MoveResult::RotatedOnly;
            }
            self.push_log("Blocked");
            MoveResult::Blocked
        }
    }

    fn apply_action(&mut self, action: Action) {
        let mut consume_turn = true;
        let mut keep_harvest_chain = false;
        match action {
            Action::Move(dx, dy) => {
                let result = self.try_move(dx, dy);
                if result == MoveResult::RotatedOnly {
                    consume_turn = false;
                }
            }
            Action::Face(dx, dy) => {
                if let Some(facing) = Facing::from_delta(dx, dy) {
                    if facing != self.facing {
                        self.facing = facing;
                        self.push_log(format!("Facing {}", self.facing.label()));
                    }
                }
                consume_turn = false;
            }
            Action::Attack => {
                keep_harvest_chain = self.player_attack();
            }
            Action::Wait => {
                self.push_log("Waited");
            }
        }
        if consume_turn {
            if !keep_harvest_chain {
                self.harvest_state = None;
            }
            self.consume_turn();
        }
    }

    fn push_log<S: Into<String>>(&mut self, msg: S) {
        self.logs.push(msg.into());
        if self.logs.len() > 300 {
            self.logs.drain(0..100);
        }
    }

    fn player_attack(&mut self) -> bool {
        let (dx, dy) = self.facing.delta();
        let tx = self.player.x + dx;
        let ty = self.player.y + dy;
        let target_idx = self
            .enemies
            .iter()
            .position(|e| e.pos.x == tx && e.pos.y == ty);

        match target_idx {
            Some(i) => {
                self.enemies[i].hp -= 1;
                if self.enemies[i].hp <= 0 {
                    let dead = self.enemies.remove(i);
                    self.push_log(format!("You defeated an enemy at ({}, {})", dead.pos.x, dead.pos.y));
                    if self.rand_u32() % 100 < 35 {
                        self.ground_items.insert((dead.pos.x, dead.pos.y), Item::Potion);
                        self.push_log("Enemy dropped a potion");
                    }
                } else {
                    self.push_log("You hit an enemy");
                }
                false
            }
            None => {
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
                                self.push_log(format!("{} broke and became {}", label, item.name()));
                            } else {
                                self.push_log(format!("{} broke", label));
                            }
                        } else {
                            self.push_log(format!("{} broke", label));
                        }
                    } else {
                        self.harvest_state = Some(HarvestState {
                            target: (tx, ty),
                            hits,
                        });
                        self.push_log(format!("Damaged {} ({}/{})", label, hits, durability));
                    }
                    true
                } else {
                    self.push_log("No enemy or harvestable target in front");
                    false
                }
            }
        }
    }

    fn has_enemy_at(&self, x: i32, y: i32) -> bool {
        self.enemies.iter().any(|e| e.pos.x == x && e.pos.y == y)
    }

    fn item_at(&self, x: i32, y: i32) -> Option<Item> {
        self.ground_items.get(&(x, y)).copied()
    }

    fn inventory_len(&self) -> usize {
        self.inventory.len()
    }

    fn inventory_item_name(&self, idx: usize) -> Option<String> {
        self.inventory.get(idx).map(InventoryItem::display_name)
    }

    fn add_item_to_inventory(&mut self, item: InventoryItem) -> bool {
        if self.inventory_full() {
            return false;
        }
        self.inventory.push(item);
        true
    }

    fn add_item_kind_to_inventory(&mut self, kind: Item) -> bool {
        self.add_item_to_inventory(InventoryItem {
            kind,
            custom_name: None,
        })
    }

    fn place_ground_item_near_player(&mut self, kind: Item) -> bool {
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
            let x = self.player.x + dx;
            let y = self.player.y + dy;
            if self.item_at(x, y).is_none() {
                self.ground_items.insert((x, y), kind);
                return true;
            }
        }
        false
    }

    fn stash_or_drop_item(&mut self, item: InventoryItem) {
        if self.add_item_to_inventory(item.clone()) {
            return;
        }
        if self.place_ground_item_near_player(item.kind) {
            self.push_log(format!("Dropped {} to ground (inventory full)", item.display_name()));
        } else {
            self.push_log(format!("Lost {} (no space)", item.display_name()));
        }
    }

    fn inventory_full(&self) -> bool {
        self.inventory.len() >= MAX_INVENTORY
    }

    fn pick_up_item_at_player(&mut self) {
        let key = (self.player.x, self.player.y);
        let picked = self.ground_items.get(&key).copied();
        if let Some(item) = picked {
            if self.inventory_full() {
                self.push_log("Inventory full (max 10)");
                return;
            }
            self.ground_items.remove(&key);
            let _ = self.add_item_kind_to_inventory(item);
            self.push_log(format!(
                "Picked up {} ({}/{})",
                item.name(),
                self.inventory.len(),
                MAX_INVENTORY
            ));
        }
    }

    fn use_inventory_item(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            self.push_log("No usable item");
            return false;
        }
        let kind = self.inventory[idx].kind;
        match kind {
            Item::Potion => {
                let item = self.inventory.remove(idx);
                let before = self.player_hp;
                self.player_hp = (self.player_hp + POTION_HEAL).min(self.player_max_hp);
                let healed = self.player_hp - before;
                if healed > 0 {
                    self.push_log(format!(
                        "Used {}: +{} HP ({}/{})",
                        item.display_name(),
                        healed,
                        self.player_hp,
                        self.player_max_hp
                    ));
                } else {
                    self.push_log(format!("Used {}, but HP is already full", item.display_name()));
                }
                true
            }
            Item::Wood => {
                self.push_log("This item cannot be used directly");
                false
            }
            Item::Stone => {
                self.push_log("This item cannot be used directly");
                false
            }
            Item::StringFiber => {
                self.push_log("This item cannot be used directly");
                false
            }
            Item::StoneAxe => {
                let item = self.inventory.remove(idx);
                let old = self.equipped_tool.replace(item);
                if let Some(prev) = old {
                    self.stash_or_drop_item(prev);
                    self.push_log("Equipped Stone Axe (swapped previous tool)");
                } else {
                    self.push_log("Equipped Stone Axe");
                }
                true
            }
        }
    }

    fn drop_inventory_item(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            return false;
        }
        let key = (self.player.x, self.player.y);
        if self.ground_items.contains_key(&key) {
            self.push_log("Cannot drop here: tile already has an item");
            return false;
        }
        let item = self.inventory.remove(idx);
        self.ground_items.insert(key, item.kind);
        self.push_log(format!("Dropped {}", item.display_name()));
        true
    }

    fn throw_inventory_item(&mut self, idx: usize) -> bool {
        if idx >= self.inventory.len() {
            return false;
        }
        let item = self.inventory.remove(idx);
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
            self.push_log(format!("Threw {} but it fell at your feet", item.display_name()));
        } else {
            self.push_log(format!("Threw {} to ({}, {})", item.display_name(), tx, ty));
        }
        self.ground_items.insert((tx, ty), item.kind);
        true
    }

    fn rename_inventory_item(&mut self, idx: usize, new_name: String) -> bool {
        if idx >= self.inventory.len() {
            return false;
        }
        let trimmed = new_name.trim().to_string();
        if trimmed.is_empty() {
            self.inventory[idx].custom_name = None;
            self.push_log("Item name reset");
        } else {
            self.inventory[idx].custom_name = Some(trimmed.clone());
            self.push_log(format!("Renamed item to \"{}\"", trimmed));
        }
        true
    }

    fn consume_turn(&mut self) {
        self.tick_enemies();
        self.turn = self.turn.saturating_add(1);
    }

    fn consume_non_attack_turn(&mut self) {
        self.harvest_state = None;
        self.consume_turn();
    }

    fn is_enemy_passable(&mut self, x: i32, y: i32, occupied: &HashMap<(i32, i32), usize>) -> bool {
        if x == self.player.x && y == self.player.y {
            return false;
        }
        self.tile(x, y).walkable() && !occupied.contains_key(&(x, y))
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
            self.enemies.push(Enemy {
                pos: Pos { x, y },
                hp: 2,
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
            self.ground_items.insert((x, y), Item::Potion);
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
        let mut attack_count = 0_u32;

        for i in 0..self.enemies.len() {
            let current = self.enemies[i].pos;
            occupied.remove(&(current.x, current.y));

            let dx = self.player.x - current.x;
            let dy = self.player.y - current.y;
            let dist2 = dx * dx + dy * dy;
            let chebyshev = dx.abs().max(dy.abs());
            if chebyshev == 1 {
                self.player_hp -= 1;
                attack_count += 1;
                occupied.insert((current.x, current.y), i);
                continue;
            }

            let candidates = if dist2 <= 64 {
                let sx = dx.signum();
                let sy = dy.signum();
                let x_first = self.rand_u32().is_multiple_of(2);
                if x_first {
                    [(sx, 0), (0, sy), (sx, sy), (0, 0), (-sx, 0)]
                } else {
                    [(0, sy), (sx, 0), (sx, sy), (0, 0), (0, -sy)]
                }
            } else {
                let r = (self.rand_u32() % 9) as i32;
                match r {
                    0 => [(1, 0), (1, 1), (0, 1), (1, -1), (0, 0)],
                    1 => [(0, 1), (-1, 1), (-1, 0), (1, 1), (0, 0)],
                    2 => [(-1, 0), (-1, -1), (0, -1), (-1, 1), (0, 0)],
                    3 => [(0, -1), (1, -1), (1, 0), (-1, -1), (0, 0)],
                    4 => [(1, 1), (1, 0), (0, 1), (-1, -1), (0, 0)],
                    5 => [(-1, 1), (-1, 0), (0, 1), (1, -1), (0, 0)],
                    6 => [(-1, -1), (-1, 0), (0, -1), (1, 1), (0, 0)],
                    7 => [(1, -1), (1, 0), (0, -1), (-1, 1), (0, 0)],
                    _ => [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)],
                }
            };

            let mut next = current;
            for (mx, my) in candidates {
                let nx = current.x + mx;
                let ny = current.y + my;
                if self.is_enemy_passable(nx, ny, &occupied) {
                    next = Pos { x: nx, y: ny };
                    break;
                }
            }

            self.enemies[i].pos = next;
            occupied.insert((next.x, next.y), i);
        }

        if attack_count > 0 {
            if self.player_hp <= 0 {
                self.push_log("You were slain");
            } else {
                self.push_log(format!(
                    "{attack_count} enemy attack(s) hit you. HP {}/{}",
                    self.player_hp, self.player_max_hp
                ));
            }
        }
    }
}

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
    let areas = Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(frame.area());
    let side_areas = Layout::vertical([
        Constraint::Length(14),
        Constraint::Min(10),
        Constraint::Length(5),
    ])
    .split(areas[1]);

    let map_block = Block::default().borders(Borders::ALL).title("Map");
    let map_inner = map_block.inner(areas[0]);
    let map_lines = build_map_lines(game, map_inner.width, map_inner.height);
    let map_widget = Paragraph::new(map_lines).block(map_block);
    frame.render_widget(map_widget, areas[0]);

    let status_widget = Paragraph::new(build_status_lines(game, esc_hold_count))
        .block(Block::default().borders(Borders::ALL).title("Status"))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(status_widget, side_areas[0]);

    let legend_widget = Paragraph::new(build_legend_lines())
        .block(Block::default().borders(Borders::ALL).title("Legend"))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(legend_widget, side_areas[1]);

    let log_height = side_areas[2].height.saturating_sub(2) as usize;
    let log_widget = Paragraph::new(build_log_lines(game, log_height))
        .block(Block::default().borders(Borders::ALL).title("Log"))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(log_widget, side_areas[2]);

    match ui_mode {
        UiMode::Normal => {}
        UiMode::Inventory { selected } => {
            let area = centered_rect(60, 65, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_inventory_lines(game, *selected))
                .block(Block::default().borders(Borders::ALL).title("Inventory"))
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::ItemMenu {
            selected,
            action_idx,
        } => {
            let area = centered_rect(40, 40, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_item_menu_lines(game, *selected, *action_idx))
                .block(Block::default().borders(Borders::ALL).title("Item Menu"))
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::RenameItem { selected, input } => {
            let area = centered_rect(55, 30, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_rename_lines(game, *selected, input))
                .block(Block::default().borders(Borders::ALL).title("Rename Item"))
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

fn build_map_lines(game: &mut Game, width: u16, height: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(height as usize);
    let center_x = (width / 2) as i32;
    let center_y = (height / 2) as i32;

    for sy in 0..height {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(width as usize);
        for sx in 0..width {
            let dx = sx as i32 - center_x;
            let dy = sy as i32 - center_y;
            let world_x = game.player.x + dx;
            let world_y = game.player.y + dy;
            let bright = is_bright_by_facing(game.facing, dx, dy);
            let dim_mod = if bright {
                Modifier::empty()
            } else {
                Modifier::DIM
            };

            let span = if dx * dx + dy * dy > VISION_RADIUS * VISION_RADIUS {
                Span::raw(" ")
            } else if sx as i32 == center_x && sy as i32 == center_y {
                Span::styled("@", Style::default().fg(Color::Red).bold())
            } else if game.has_enemy_at(world_x, world_y) {
                let enemy_color = if bright {
                    Color::LightRed
                } else {
                    Color::Indexed(52)
                };
                Span::styled("E", Style::default().fg(enemy_color).add_modifier(dim_mod))
            } else if let Some(item) = game.item_at(world_x, world_y) {
                let item_color = if bright {
                    item.color()
                } else {
                    Color::Indexed(94)
                };
                Span::styled(
                    item.glyph().to_string(),
                    Style::default().fg(item_color).add_modifier(dim_mod),
                )
            } else {
                let t = game.tile(world_x, world_y);
                let fg = if bright { t.color() } else { shadow_color(t) };
                Span::styled(
                    t.glyph().to_string(),
                    Style::default().fg(fg).add_modifier(dim_mod),
                )
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn build_status_lines(game: &Game, esc_hold_count: u8) -> Vec<Line<'static>> {
    let esc_line = if esc_hold_count == 0 {
        "Hold ESC to quit".to_string()
    } else {
        format!("Hold ESC to quit ({esc_hold_count}/{ESC_HOLD_STEPS})")
    };

    let equipped = game
        .equipped_tool
        .as_ref()
        .map(InventoryItem::display_name)
        .unwrap_or_else(|| "(none)".to_string());

    vec![
        Line::from(format!("HP: {}/{}", game.player_hp.max(0), game.player_max_hp)),
        Line::from(format!("Turn: {}", game.turn)),
        Line::from(format!("Enemies: {}", game.enemies.len())),
        Line::from(format!("Items: {}/{}", game.inventory_len(), MAX_INVENTORY)),
        Line::from(format!("Equipped: {}", equipped)),
        Line::from(format!(
            "Pos: ({}, {})",
            game.player.x, game.player.y
        )),
        Line::from(format!("Facing: {} {}", game.facing.label(), game.facing.glyph())),
        Line::from(format!("Chunks: {}", game.world.chunks.len())),
        Line::raw(""),
        Line::from("W/A/S/D : Move"),
        Line::from("Q/E/Z/C : Diagonal"),
        Line::from("Arrows  : Move"),
        Line::from("Shift+Move: Face only"),
        Line::from("F       : Attack"),
        Line::from("I       : Inventory"),
        Line::from("Tab     : Crafting"),
        Line::from(".       : Wait"),
        Line::from(esc_line),
    ]
}

fn build_legend_lines() -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("@ : Player"), Line::from("E : Enemy")];
    for item in [
        Item::Potion,
        Item::Wood,
        Item::Stone,
        Item::StringFiber,
        Item::StoneAxe,
    ] {
        lines.push(Line::from(format!(
            "{} : {}",
            item.glyph(),
            item_meta(item).legend
        )));
    }
    for tile in [
        Tile::Abyss,
        Tile::DeepWater,
        Tile::ShallowWater,
        Tile::Sand,
        Tile::Grass,
        Tile::Forest,
        Tile::Mountain,
        Tile::Rock,
        Tile::Wall,
    ] {
        lines.push(Line::from(format!(
            "{} : {}",
            tile.glyph(),
            tile_meta(tile).legend
        )));
    }
    lines
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

fn build_log_lines(game: &Game, max_lines: usize) -> Vec<Line<'static>> {
    if game.logs.is_empty() {
        return vec![Line::from("")];
    }
    let keep = max_lines.max(1);
    let start = game.logs.len().saturating_sub(keep);
    game.logs[start..]
        .iter()
        .map(|entry| Line::from(entry.clone()))
        .collect()
}

fn build_inventory_lines(game: &Game, selected: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!("Items: {}/{}", game.inventory_len(), MAX_INVENTORY)),
        Line::from("Enter: item menu  Esc/I: close"),
        Line::raw(""),
    ];
    if game.inventory.is_empty() {
        lines.push(Line::from("(empty)"));
        return lines;
    }
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
            Span::raw(item.display_name()),
        ]));
    }
    lines
}

fn build_item_menu_lines(game: &Game, selected: usize, action_idx: usize) -> Vec<Line<'static>> {
    let item_name = game
        .inventory_item_name(selected)
        .unwrap_or_else(|| "(missing)".to_string());
    let mut lines = vec![
        Line::from(format!("Item: {}", item_name)),
        Line::from("Up/Down: choose  Enter: select  Esc: back"),
        Line::raw(""),
    ];
    for (idx, action) in ITEM_MENU_ACTIONS.iter().enumerate() {
        let marker = if idx == action_idx { ">" } else { " " };
        lines.push(Line::from(format!("{} {}", marker, action.label())));
    }
    lines
}

fn build_rename_lines(game: &Game, selected: usize, input: &str) -> Vec<Line<'static>> {
    let current = game
        .inventory_item_name(selected)
        .unwrap_or_else(|| "(missing)".to_string());
    vec![
        Line::from(format!("Current: {}", current)),
        Line::from("Type new name, Enter: confirm, Esc: cancel"),
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
    let container = Block::default().borders(Borders::ALL).title("Crafting (3x3)");
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
        Line::from("Grid: move + Enter to choose item | Inventory: choose + Enter to place"),
        Line::from("X craft, Backspace remove from cell, Esc/Tab close"),
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

    frame.render_widget(Paragraph::new(Line::from("Inventory:")), sub[2]);

    let mut inv_spans: Vec<Span<'static>> = Vec::new();
    if game.inventory.is_empty() {
        inv_spans.push(Span::raw("(empty)"));
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
            inv_spans.push(Span::raw(" "));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(inv_spans)), sub[3]);

    let selected_name = game
        .inventory
        .get(selected_inv)
        .map(InventoryItem::display_name)
        .unwrap_or_else(|| "(none)".to_string());
    let result_text = match find_recipe(grid) {
        Some(recipe) => format!("Result: {}", recipe.label),
        None => "Result: (no recipe)".to_string(),
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!("Selected: {}", selected_name)),
            Line::from(result_text),
        ]),
        sub[4],
    );
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
    if let Err(e) = run() {
        eprintln!("{e}");
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
        game.push_log("No matching recipe");
        return false;
    };
    let result = recipe.result;

    for slot in grid.iter_mut() {
        *slot = None;
    }

    if game.add_item_kind_to_inventory(result) {
        game.push_log(format!("Crafted {}", recipe.label));
    } else if game.place_ground_item_near_player(result) {
        game.push_log(format!(
            "Crafted {}, but inventory full so it dropped nearby",
            recipe.label
        ));
    } else {
        game.push_log(format!("Crafted {} but it was lost", recipe.label));
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
        let picked = game.inventory.remove(selected);
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
        game.push_log("No inventory item selected");
    }
}

fn run() -> io::Result<()> {
    let _guard = TerminalGuard::new()?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut game = Game::new(12345);
    let mut esc_hold_count: u8 = 0;
    let mut ui_mode = UiMode::Normal;

    loop {
        terminal.draw(|frame| render_ui(frame, &mut game, esc_hold_count, &ui_mode))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        let ev = event::read()?;
        if let Event::Key(key) = ev {
            match &mut ui_mode {
                UiMode::Normal => {
                    if key.code == KeyCode::Esc {
                        if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                            esc_hold_count = esc_hold_count.saturating_add(1);
                            game.push_log(format!(
                                "Hold ESC to quit ({}/{})",
                                esc_hold_count, ESC_HOLD_STEPS
                            ));
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

                    if matches!(key.code, KeyCode::Char('i') | KeyCode::Char('I')) {
                        ui_mode = UiMode::Inventory { selected: 0 };
                        continue;
                    }
                    if key.code == KeyCode::Tab {
                        ui_mode = UiMode::Crafting {
                            cursor: 0,
                            selected_inv: 0,
                            focus: CraftFocus::Grid,
                            grid: std::array::from_fn(|_| None),
                        };
                        continue;
                    }

                    let action = if let Some((dx, dy)) = movement_delta(key.code) {
                        if shift_only {
                            Some(Action::Face(dx, dy))
                        } else {
                            Some(Action::Move(dx, dy))
                        }
                    } else {
                        match key.code {
                            KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                                'f' => Some(Action::Attack),
                                '.' => Some(Action::Wait),
                                _ => None,
                            },
                            _ => None,
                        }
                    };

                    if let Some(action) = action {
                        game.apply_action(action);
                        if game.player_hp <= 0 {
                            break;
                        }
                    }
                }
                UiMode::Inventory { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let len = game.inventory_len();
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('i') | KeyCode::Char('I') => {
                            ui_mode = UiMode::Normal;
                        }
                        KeyCode::Up => {
                            if len > 0 {
                                *selected = selected.saturating_sub(1);
                            }
                        }
                        KeyCode::Down => {
                            if len > 0 {
                                *selected = (*selected + 1).min(len - 1);
                            }
                        }
                        KeyCode::Enter => {
                            if len > 0 {
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
                        ui_mode = UiMode::Inventory { selected: 0 };
                        continue;
                    }
                    *selected = (*selected).min(len - 1);
                    match key.code {
                        KeyCode::Esc => {
                            ui_mode = UiMode::Inventory {
                                selected: *selected,
                            };
                        }
                        KeyCode::Up => {
                            *action_idx = action_idx.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            *action_idx = (*action_idx + 1).min(ITEM_MENU_ACTIONS.len() - 1);
                        }
                        KeyCode::Enter => {
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
                                    if game.drop_inventory_item(item_idx) {
                                        game.consume_non_attack_turn();
                                    }
                                    let next_len = game.inventory_len();
                                    if next_len == 0 {
                                        ui_mode = UiMode::Normal;
                                    } else {
                                        ui_mode = UiMode::Inventory {
                                            selected: item_idx.min(next_len - 1),
                                        };
                                    }
                                }
                                ItemMenuAction::Throw => {
                                    if game.throw_inventory_item(item_idx) {
                                        game.consume_non_attack_turn();
                                    }
                                    let next_len = game.inventory_len();
                                    if next_len == 0 {
                                        ui_mode = UiMode::Normal;
                                    } else {
                                        ui_mode = UiMode::Inventory {
                                            selected: item_idx.min(next_len - 1),
                                        };
                                    }
                                }
                                ItemMenuAction::Use => {
                                    if game.use_inventory_item(item_idx) {
                                        game.consume_non_attack_turn();
                                    }
                                    let next_len = game.inventory_len();
                                    if next_len == 0 {
                                        ui_mode = UiMode::Normal;
                                    } else {
                                        ui_mode = UiMode::Inventory {
                                            selected: item_idx.min(next_len - 1),
                                        };
                                    }
                                }
                            }
                            if game.player_hp <= 0 {
                                break;
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
                        ui_mode = UiMode::Normal;
                        continue;
                    }
                    let item_idx = (*selected).min(len - 1);
                    match key.code {
                        KeyCode::Esc => {
                            ui_mode = UiMode::ItemMenu {
                                selected: item_idx,
                                action_idx: 0,
                            };
                        }
                        KeyCode::Enter => {
                            let name = input.clone();
                            let _ = game.rename_inventory_item(item_idx, name);
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
                        KeyCode::Esc | KeyCode::Tab => {
                            if *focus == CraftFocus::Inventory {
                                *focus = CraftFocus::Grid;
                            } else {
                                close_crafting_mode(&mut game, grid);
                                ui_mode = UiMode::Normal;
                            }
                        }
                        KeyCode::Up => {
                            if *focus == CraftFocus::Grid {
                                *cursor = move_cursor_3x3(*cursor, 0, -1);
                            }
                        }
                        KeyCode::Down => {
                            if *focus == CraftFocus::Grid {
                                *cursor = move_cursor_3x3(*cursor, 0, 1);
                            }
                        }
                        KeyCode::Left => {
                            if *focus == CraftFocus::Grid {
                                *cursor = move_cursor_3x3(*cursor, -1, 0);
                            } else if inv_len > 0 {
                                *selected_inv = selected_inv.saturating_sub(1);
                            }
                        }
                        KeyCode::Right => {
                            if *focus == CraftFocus::Grid {
                                *cursor = move_cursor_3x3(*cursor, 1, 0);
                            } else if inv_len > 0 {
                                *selected_inv = (*selected_inv + 1).min(inv_len - 1);
                            }
                        }
                        KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                            'w' => {
                                if *focus == CraftFocus::Grid {
                                    *cursor = move_cursor_3x3(*cursor, 0, -1);
                                }
                            }
                            's' => {
                                if *focus == CraftFocus::Grid {
                                    *cursor = move_cursor_3x3(*cursor, 0, 1);
                                }
                            }
                            'a' => {
                                if *focus == CraftFocus::Grid {
                                    *cursor = move_cursor_3x3(*cursor, -1, 0);
                                } else if inv_len > 0 {
                                    *selected_inv = selected_inv.saturating_sub(1);
                                }
                            }
                            'd' => {
                                if *focus == CraftFocus::Grid {
                                    *cursor = move_cursor_3x3(*cursor, 1, 0);
                                } else if inv_len > 0 {
                                    *selected_inv = (*selected_inv + 1).min(inv_len - 1);
                                }
                            }
                            'x' => {
                                if *focus == CraftFocus::Grid && execute_crafting(&mut game, grid) {
                                    game.consume_non_attack_turn();
                                    if game.player_hp <= 0 {
                                        break;
                                    }
                                }
                            }
                            ' ' => {
                                if *focus == CraftFocus::Grid {
                                    let idx = *cursor;
                                    if let Some(item) = grid[idx].take() {
                                        game.stash_or_drop_item(item);
                                    } else if inv_len > 0 {
                                        *focus = CraftFocus::Inventory;
                                    } else {
                                        game.push_log("No inventory item to place");
                                    }
                                } else {
                                    place_inventory_item_into_grid(
                                        &mut game,
                                        grid,
                                        *cursor,
                                        selected_inv,
                                    );
                                    *focus = CraftFocus::Grid;
                                }
                            }
                            _ => {}
                        },
                        KeyCode::Enter => {
                            if *focus == CraftFocus::Grid {
                                let idx = *cursor;
                                if let Some(item) = grid[idx].take() {
                                    game.stash_or_drop_item(item);
                                } else if inv_len > 0 {
                                    *focus = CraftFocus::Inventory;
                                } else {
                                    game.push_log("No inventory item to place");
                                }
                            } else {
                                place_inventory_item_into_grid(&mut game, grid, *cursor, selected_inv);
                                *focus = CraftFocus::Grid;
                            }
                        }
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
            }
        }
    }

    Ok(())
}
