use std::collections::HashMap;

use ratatui::prelude::Color;
use serde::{Deserialize, Serialize};

use crate::defs::{Faction, creature_meta, defs, tile_meta};
use crate::text::{tr, trf};
use crate::{Facing, InventoryItem, Item, Tile};

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
    pub(crate) pos: Pos,
    pub(crate) hp: i32,
    pub(crate) creature_id: String,
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
    pub(crate) ttl_frames: u8,
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
    const NAMES: [&str; 16] = [
        "Ash Barrens",
        "Salt Marsh",
        "Green Fields",
        "Luminous Grove",
        "Frost Flats",
        "Fog Meadow",
        "Woodland",
        "Rainforest",
        "Stone Steppe",
        "Highland",
        "Alpine Forest",
        "Cloud Ridge",
        "Obsidian Waste",
        "Crag Depths",
        "Spire Peaks",
        "Elder Summit",
    ];
    NAMES[id.0 as usize]
}

fn biome_enemy_multiplier(biome: BiomeId, creature_id: &str) -> u32 {
    let bx = biome.0 % 4;
    let by = biome.0 / 4;
    let edge = bx == 0 || bx == 3 || by == 0 || by == 3;
    match creature_id {
        "slime" => match (bx, by) {
            (1, 0) | (2, 0) | (1, 1) | (2, 1) => 220,
            (1, 2) | (2, 2) => 140,
            _ => 70,
        },
        "wolf" => match (bx, by) {
            (2, 1) | (3, 1) | (2, 2) | (3, 2) => 220,
            (1, 1) | (1, 2) => 130,
            _ => 55,
        },
        "bat" => match (bx, by) {
            (2, 2) | (3, 2) | (2, 3) | (3, 3) => 220,
            (1, 2) | (1, 3) => 125,
            _ => 55,
        },
        "golem" => {
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
            (Item::Herb, 30),
            (Item::Potion, 15),
            (Item::Wood, 10),
            (Item::Hide, 5),
        ],
        // greener
        (2, 1) | (1, 2) | (2, 2) => &[
            (Item::Wood, 38),
            (Item::StringFiber, 20),
            (Item::Herb, 18),
            (Item::Potion, 14),
            (Item::Hide, 10),
        ],
        // rocky / high
        (3, 2) | (2, 3) | (3, 3) => &[
            (Item::Stone, 36),
            (Item::IronIngot, 28),
            (Item::Potion, 14),
            (Item::Wood, 8),
            (Item::Elixir, 6),
            (Item::Herb, 8),
        ],
        // mixed
        _ => &[
            (Item::Potion, 20),
            (Item::Wood, 24),
            (Item::Stone, 22),
            (Item::StringFiber, 12),
            (Item::Herb, 12),
            (Item::IronIngot, 6),
            (Item::Hide, 4),
        ],
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
    facing: Facing,
    player_hp: i32,
    player_max_hp: i32,
    inventory: Vec<InventoryItem>,
    equipped_tool: Option<InventoryItem>,
    enemies: Vec<EnemyState>,
    ground_items: Vec<GroundItemState>,
    harvest_state: Option<HarvestStateState>,
    rng_state: u64,
    turn: u64,
    logs: Vec<String>,
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
    pos: PosState,
    hp: i32,
    creature_id: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct GroundItemState {
    x: i32,
    y: i32,
    item: Item,
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
    pub(crate) facing: Facing,
    pub(crate) player_hp: i32,
    pub(crate) player_max_hp: i32,
    pub(crate) inventory: Vec<InventoryItem>,
    pub(crate) equipped_tool: Option<InventoryItem>,
    pub(crate) enemies: Vec<Enemy>,
    pub(crate) ground_items: HashMap<(i32, i32), Item>,
    pub(crate) attack_effects: Vec<AttackEffect>,
    harvest_state: Option<HarvestState>,
    rng_state: u64,
    pub(crate) turn: u64,
    pub(crate) logs: Vec<String>,
}

impl Game {
    pub(crate) fn new(seed: u64) -> Self {
        let mut game = Self {
            world: World::new(seed),
            player: Pos { x: 0, y: 0 },
            facing: Facing::S,
            player_hp: creature_meta("player").hp,
            player_max_hp: creature_meta("player").hp,
            inventory: Vec::new(),
            equipped_tool: None,
            enemies: Vec::new(),
            ground_items: HashMap::new(),
            attack_effects: Vec::new(),
            harvest_state: None,
            rng_state: seed ^ 0xA5A5_5A5A_DEAD_BEEF,
            turn: 0,
            logs: vec![tr("game.start").to_string()],
        };
        game.player = game.find_spawn();
        game.spawn_enemies(12);
        game.spawn_items(10);
        game
    }

    pub(crate) fn tile(&mut self, x: i32, y: i32) -> Tile {
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
                self.push_log(trf(
                    "game.facing",
                    &[("label", self.facing.label().to_string())],
                ));
                return MoveResult::RotatedOnly;
            }
            self.push_log(tr("game.enemy_blocks"));
            return MoveResult::Blocked;
        }
        if self.tile(nx, ny).walkable() {
            self.player = Pos { x: nx, y: ny };
            self.push_log(trf(
                "game.moved",
                &[("x", nx.to_string()), ("y", ny.to_string())],
            ));
            self.pick_up_item_at_player();
            MoveResult::Moved
        } else {
            if self.facing != old_facing {
                self.push_log(trf(
                    "game.facing",
                    &[("label", self.facing.label().to_string())],
                ));
                return MoveResult::RotatedOnly;
            }
            self.push_log(tr("game.blocked"));
            MoveResult::Blocked
        }
    }

    pub(crate) fn apply_action(&mut self, action: Action) {
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
                        self.push_log(trf(
                            "game.facing",
                            &[("label", self.facing.label().to_string())],
                        ));
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

    fn player_attack(&mut self) -> bool {
        let (dx, dy) = self.facing.delta();
        let tx = self.player.x + dx;
        let ty = self.player.y + dy;
        self.push_attack_effect(self.player, Pos { x: tx, y: ty });
        let target_idx = self
            .enemies
            .iter()
            .position(|e| e.pos.x == tx && e.pos.y == ty);

        match target_idx {
            Some(i) => {
                let enemy_def = creature_meta(&self.enemies[i].creature_id).defense;
                let damage = calc_damage(self.player_attack_power(), enemy_def);
                self.enemies[i].hp -= damage;
                if self.enemies[i].hp <= 0 {
                    let dead = self.enemies.remove(i);
                    self.push_log(trf(
                        "game.you_defeated",
                        &[
                            ("x", dead.pos.x.to_string()),
                            ("y", dead.pos.y.to_string()),
                        ],
                    ));
                    if self.rand_u32() % 100 < 60 {
                        let drop = match self.rand_u32() % 100 {
                            0..=34 => Item::Potion,
                            35..=69 => Item::Herb,
                            70..=84 => Item::Hide,
                            85..=94 => Item::IronIngot,
                            _ => Item::Elixir,
                        };
                        self.ground_items.insert((dead.pos.x, dead.pos.y), drop);
                        self.push_log(trf(
                            "game.enemy_drop_item",
                            &[("item", drop.name().to_string())],
                        ));
                    }
                } else {
                    self.push_log(trf(
                        "game.you_hit_enemy",
                        &[("damage", damage.to_string())],
                    ));
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
                                self.push_log(trf(
                                    "game.broke_to_item",
                                    &[
                                        ("target", label.to_string()),
                                        ("item", item.name().to_string()),
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

    pub(crate) fn inventory_len(&self) -> usize {
        self.inventory.len()
    }

    pub(crate) fn inventory_item_name(&self, idx: usize) -> Option<String> {
        self.inventory.get(idx).map(InventoryItem::display_name)
    }

    fn add_item_to_inventory(&mut self, item: InventoryItem) -> bool {
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
        })
    }

    fn inventory_full(&self) -> bool {
        self.inventory.len() >= crate::MAX_INVENTORY
    }

    pub(crate) fn place_ground_item_near_player(&mut self, kind: Item) -> bool {
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
            self.push_log(trf(
                "game.picked",
                &[
                    ("item", item.name().to_string()),
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
                let item = self.inventory.remove(idx);
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
                let item = self.inventory.remove(idx);
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
                let item = self.inventory.remove(idx);
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
            Item::StoneAxe | Item::IronSword | Item::IronPickaxe => {
                let item = self.inventory.remove(idx);
                let equipped_name = item.display_name();
                let old = self.equipped_tool.replace(item);
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
            Item::Wood | Item::Stone | Item::StringFiber | Item::IronIngot | Item::Hide => {
                self.push_log(tr("game.cannot_use_direct"));
                false
            }
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
        let item = self.inventory.remove(idx);
        self.ground_items.insert(key, item.kind);
        self.push_log(trf("game.dropped", &[("item", item.display_name())]));
        true
    }

    pub(crate) fn throw_inventory_item(&mut self, idx: usize) -> bool {
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
            self.push_log(trf("game.throw_feet", &[("item", item.display_name())]));
        } else {
            self.push_log(trf(
                "game.throw_to",
                &[
                    ("item", item.display_name()),
                    ("x", tx.to_string()),
                    ("y", ty.to_string()),
                ],
            ));
        }
        self.ground_items.insert((tx, ty), item.kind);
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
            let biome = self.world.biome_id_at(x, y);
            let spawnables: Vec<(&str, u32)> = defs()
                .creatures
                .iter()
                .filter_map(|(id, c)| {
                    if c.faction == Faction::Hostile && c.spawn_weight > 0 {
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
                return;
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
            let cdef = creature_meta(chosen);
            self.enemies.push(Enemy {
                pos: Pos { x, y },
                hp: cdef.hp,
                creature_id: chosen.to_string(),
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
        let mut attack_count = 0_u32;

        for i in 0..self.enemies.len() {
            let current = self.enemies[i].pos;
            occupied.remove(&(current.x, current.y));

            let dx = self.player.x - current.x;
            let dy = self.player.y - current.y;
            let dist2 = dx * dx + dy * dy;
            let chebyshev = dx.abs().max(dy.abs());
            if chebyshev == 1 {
                let enemy_atk = creature_meta(&self.enemies[i].creature_id).attack;
                let damage = calc_damage(enemy_atk, self.player_defense());
                self.player_hp -= damage;
                attack_count += 1;
                self.push_attack_effect(current, self.player);
                self.push_log(trf(
                    "game.enemy_hit_you",
                    &[
                        ("x", current.x.to_string()),
                        ("y", current.y.to_string()),
                        ("damage", damage.to_string()),
                    ],
                ));
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

        if attack_count > 0 && self.player_hp <= 0 {
            self.push_log(tr("game.you_slain"));
        }
    }

    pub(crate) fn player_attack_power(&self) -> i32 {
        creature_meta("player").attack + self.equipped_attack_bonus()
    }

    pub(crate) fn player_defense(&self) -> i32 {
        creature_meta("player").defense
    }

    pub(crate) fn generated_chunks(&self) -> usize {
        self.world.chunks.len()
    }

    pub(crate) fn current_biome_name(&mut self) -> &'static str {
        self.world.biome_name_at(self.player.x, self.player.y)
    }

    pub(crate) fn biome_index_at(&mut self, x: i32, y: i32) -> u8 {
        self.world.biome_id_at(x, y).0
    }

    fn equipped_attack_bonus(&self) -> i32 {
        match self.equipped_tool.as_ref().map(|t| t.kind) {
            Some(Item::StoneAxe) => 2,
            Some(Item::IronPickaxe) => 3,
            Some(Item::IronSword) => 4,
            _ => 0,
        }
    }

    fn push_attack_effect(&mut self, from: Pos, to: Pos) {
        self.attack_effects.push(AttackEffect {
            from,
            to,
            ttl_frames: 1,
        });
    }

    pub(crate) fn advance_effects(&mut self) {
        for fx in &mut self.attack_effects {
            fx.ttl_frames = fx.ttl_frames.saturating_sub(1);
        }
        self.attack_effects.retain(|fx| fx.ttl_frames > 0);
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
            facing: self.facing,
            player_hp: self.player_hp,
            player_max_hp: self.player_max_hp,
            inventory: self.inventory.clone(),
            equipped_tool: self.equipped_tool.clone(),
            enemies: self
                .enemies
                .iter()
                .map(|e| EnemyState {
                    pos: PosState {
                        x: e.pos.x,
                        y: e.pos.y,
                    },
                    hp: e.hp,
                    creature_id: e.creature_id.clone(),
                })
                .collect(),
            ground_items,
            harvest_state: self.harvest_state.map(|h| HarvestStateState {
                x: h.target.0,
                y: h.target.1,
                hits: h.hits,
            }),
            rng_state: self.rng_state,
            turn: self.turn,
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

        Ok(Self {
            world: World {
                seed: snapshot.seed,
                chunks,
            },
            player: Pos {
                x: snapshot.player.x,
                y: snapshot.player.y,
            },
            facing: snapshot.facing,
            player_hp: snapshot.player_hp,
            player_max_hp: snapshot.player_max_hp,
            inventory: snapshot.inventory,
            equipped_tool: snapshot.equipped_tool,
            enemies: snapshot
                .enemies
                .into_iter()
                .map(|e| Enemy {
                    pos: Pos {
                        x: e.pos.x,
                        y: e.pos.y,
                    },
                    hp: e.hp,
                    creature_id: e.creature_id,
                })
                .collect(),
            ground_items,
            attack_effects: Vec::new(),
            harvest_state: snapshot.harvest_state.map(|h| HarvestState {
                target: (h.x, h.y),
                hits: h.hits,
            }),
            rng_state: snapshot.rng_state,
            turn: snapshot.turn,
            logs: snapshot.logs,
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
