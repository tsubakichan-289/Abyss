use std::collections::HashMap;
use std::sync::OnceLock;

use ratatui::prelude::Color;
use serde::Deserialize;

use crate::{Item, Tile};

#[derive(Deserialize)]
struct GameDataFile {
    tiles: Vec<TileDefRaw>,
    items: Vec<ItemDefRaw>,
    recipes: Vec<RecipeDefRaw>,
    creatures: Vec<CreatureDefRaw>,
    #[serde(default)]
    rituals: RitualsRaw,
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
    status: String,
    description: String,
}

#[derive(Deserialize)]
struct RecipeDefRaw {
    result: String,
    label: Option<String>,
    inputs: Vec<String>,
}

#[derive(Deserialize)]
struct CreatureDefRaw {
    id: String,
    name: String,
    faction: String,
    glyph: String,
    color: u8,
    hp: i32,
    attack: i32,
    defense: i32,
    agility: i32,
    spawn_weight: Option<u32>,
    #[serde(default)]
    loot: Vec<CreatureLootRaw>,
}

#[derive(Deserialize)]
struct CreatureLootRaw {
    id: String,
    carry_chance: u8,
    #[serde(default = "default_drop_chance")]
    drop_chance: u8,
    #[serde(default)]
    equip_as_weapon: bool,
}

#[derive(Default, Deserialize)]
struct RitualsRaw {
    forge_scroll: Option<ForgeScrollRitualRaw>,
}

#[derive(Deserialize)]
struct ForgeScrollRitualRaw {
    pattern: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct TileDef {
    pub(crate) glyph: char,
    pub(crate) color: Color,
    pub(crate) shadow_color: Color,
    pub(crate) walkable: bool,
    pub(crate) legend: String,
    pub(crate) harvest_hits: Option<u8>,
    pub(crate) harvest_drop: Option<Item>,
    pub(crate) harvest_drop_chance: u8,
    pub(crate) harvest_replace: Option<Tile>,
    pub(crate) harvest_label: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ItemDef {
    pub(crate) name: String,
    pub(crate) glyph: char,
    pub(crate) color: Color,
    pub(crate) status: String,
    pub(crate) description: String,
}

#[derive(Clone)]
pub(crate) struct RecipeDef {
    pub(crate) result: Item,
    pub(crate) label: String,
    pub(crate) inputs: [Option<Item>; 9],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Faction {
    Ally,
    Hostile,
    Neutral,
}

impl Faction {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "ally" => Some(Self::Ally),
            "hostile" => Some(Self::Hostile),
            "neutral" => Some(Self::Neutral),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CreatureDef {
    pub(crate) name: String,
    pub(crate) faction: Faction,
    pub(crate) glyph: char,
    pub(crate) color: Color,
    pub(crate) hp: i32,
    pub(crate) attack: i32,
    pub(crate) defense: i32,
    pub(crate) agility: i32,
    pub(crate) spawn_weight: u32,
    pub(crate) loot: Vec<CreatureLootDef>,
}

#[derive(Clone, Copy)]
pub(crate) struct CreatureLootDef {
    pub(crate) item: Item,
    pub(crate) carry_chance: u8,
    pub(crate) drop_chance: u8,
    pub(crate) equip_as_weapon: bool,
}

fn default_drop_chance() -> u8 {
    100
}

fn is_weapon_item(item: Item) -> bool {
    matches!(item, Item::StoneAxe | Item::IronSword | Item::IronPickaxe)
}

pub(crate) struct GameDefs {
    pub(crate) tiles: HashMap<String, TileDef>,
    pub(crate) items: HashMap<String, ItemDef>,
    pub(crate) recipes: Vec<RecipeDef>,
    pub(crate) creatures: HashMap<String, CreatureDef>,
    pub(crate) forge_scroll_pattern: Vec<(i32, i32, Item)>,
}

pub(crate) fn defs() -> &'static GameDefs {
    static DEFS: OnceLock<GameDefs> = OnceLock::new();
    DEFS.get_or_init(load_defs)
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

fn default_forge_scroll_pattern() -> Vec<(i32, i32, Item)> {
    vec![
        (-1, -1, Item::Wood),
        (0, -1, Item::Stone),
        (1, -1, Item::Wood),
        (-1, 0, Item::Stone),
        (1, 0, Item::Stone),
        (-1, 1, Item::Wood),
        (0, 1, Item::Stone),
        (1, 1, Item::Wood),
    ]
}

fn parse_forge_scroll_pattern(raw: Option<ForgeScrollRitualRaw>) -> Vec<(i32, i32, Item)> {
    let Some(raw) = raw else {
        return default_forge_scroll_pattern();
    };
    assert!(
        raw.pattern.len() == 3,
        "rituals.forge_scroll.pattern must have exactly 3 rows"
    );
    let mut out: Vec<(i32, i32, Item)> = Vec::new();
    let mut center_seen = false;
    for (y, row) in raw.pattern.iter().enumerate() {
        let cells: Vec<char> = row.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            cells.len() == 3,
            "rituals.forge_scroll.pattern row {} must contain exactly 3 non-space chars",
            y
        );
        for (x, ch) in cells.iter().enumerate() {
            let dx = x as i32 - 1;
            let dy = y as i32 - 1;
            match *ch {
                'w' | 'W' => out.push((dx, dy, Item::Wood)),
                's' | 'S' => out.push((dx, dy, Item::Stone)),
                '@' => {
                    assert!(
                        dx == 0 && dy == 0,
                        "rituals.forge_scroll.pattern center '@' must be at middle cell"
                    );
                    center_seen = true;
                }
                '.' | '-' | '_' => {}
                _ => panic!(
                    "rituals.forge_scroll.pattern unknown symbol '{}' (use w/s/@/.)",
                    ch
                ),
            }
        }
    }
    assert!(
        center_seen,
        "rituals.forge_scroll.pattern must contain center '@'"
    );
    out
}

fn load_defs() -> GameDefs {
    let raw = include_str!("../data/game_data.toml");
    let parsed: GameDataFile = toml::from_str(raw).expect("failed to parse data/game_data.toml");

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
                status: it.status,
                description: it.description,
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
        assert!(
            r.inputs.len() == 9,
            "recipe '{}' must have 9 inputs",
            r.result
        );
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

    let mut creatures = HashMap::new();
    for c in parsed.creatures {
        let faction = Faction::from_key(&c.faction)
            .unwrap_or_else(|| panic!("unknown creature faction '{}'", c.faction));
        let mut loot_defs: Vec<CreatureLootDef> = Vec::new();
        for loot in c.loot {
            let item = Item::from_key(&loot.id)
                .unwrap_or_else(|| panic!("unknown creature loot item '{}'", loot.id));
            assert!(
                loot.carry_chance <= 100,
                "creature '{}' loot '{}' carry_chance must be 0..=100",
                c.id,
                loot.id
            );
            assert!(
                loot.drop_chance <= 100,
                "creature '{}' loot '{}' drop_chance must be 0..=100",
                c.id,
                loot.id
            );
            if loot.equip_as_weapon {
                assert!(
                    is_weapon_item(item),
                    "creature '{}' loot '{}' marked equip_as_weapon but item is not a weapon",
                    c.id,
                    loot.id
                );
            }
            loot_defs.push(CreatureLootDef {
                item,
                carry_chance: loot.carry_chance,
                drop_chance: loot.drop_chance,
                equip_as_weapon: loot.equip_as_weapon,
            });
        }
        creatures.insert(
            c.id.clone(),
            CreatureDef {
                name: c.name,
                faction,
                glyph: parse_single_char(&c.glyph, "creature", &c.id),
                color: Color::Indexed(c.color),
                hp: c.hp,
                attack: c.attack,
                defense: c.defense,
                agility: c.agility,
                spawn_weight: c.spawn_weight.unwrap_or(0),
                loot: loot_defs,
            },
        );
    }
    assert!(
        creatures.contains_key("player"),
        "creatures must define id='player'"
    );

    GameDefs {
        tiles,
        items,
        recipes,
        creatures,
        forge_scroll_pattern: parse_forge_scroll_pattern(parsed.rituals.forge_scroll),
    }
}

pub(crate) fn tile_meta(tile: Tile) -> &'static TileDef {
    let key = tile.key();
    defs()
        .tiles
        .get(key)
        .unwrap_or_else(|| panic!("tile '{}' missing in data file", key))
}

pub(crate) fn item_meta(item: Item) -> &'static ItemDef {
    let key = item.key();
    defs()
        .items
        .get(key)
        .unwrap_or_else(|| panic!("item '{}' missing in data file", key))
}

pub(crate) fn creature_meta(id: &str) -> &'static CreatureDef {
    defs()
        .creatures
        .get(id)
        .unwrap_or_else(|| panic!("creature '{}' missing in data file", id))
}

pub(crate) fn forge_scroll_pattern() -> &'static [(i32, i32, Item)] {
    defs().forge_scroll_pattern.as_slice()
}

pub(crate) fn forge_scroll_pattern_lines() -> Vec<String> {
    let mut grid = [['.'; 3]; 3];
    grid[1][1] = '@';
    for &(dx, dy, item) in forge_scroll_pattern() {
        let x = (dx + 1) as usize;
        let y = (dy + 1) as usize;
        if x >= 3 || y >= 3 {
            continue;
        }
        grid[y][x] = match item {
            Item::Wood => 'w',
            Item::Stone => 's',
            _ => '?',
        };
    }
    grid.iter()
        .map(|row| format!("{} {} {}", row[0], row[1], row[2]))
        .collect()
}
