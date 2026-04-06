use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::Item;
use crate::defs::{Faction, defs};

#[derive(Deserialize)]
struct BiomesFile {
    #[serde(default)]
    biome_sets: Vec<BiomeSetRaw>,
}

#[derive(Deserialize)]
struct BiomeSetRaw {
    id: String,
    #[serde(default)]
    biomes: Vec<u8>,
}

#[derive(Deserialize)]
struct FloorsFile {
    #[serde(default)]
    floor_profiles: Vec<FloorProfileRaw>,
}

#[derive(Deserialize)]
struct FloorProfileRaw {
    id: String,
    biome_set: String,
    #[serde(default = "default_map_pattern")]
    map_pattern: String,
    #[serde(default = "default_terrain_theme")]
    terrain_theme: String,
    #[serde(default = "default_stairs_mode")]
    stairs_mode: String,
    #[serde(default = "default_special_facility_mode")]
    special_facility_mode: String,
    #[serde(default)]
    enemy_pool: Vec<WeightedEntryRaw>,
    #[serde(default)]
    drop_pool: Vec<WeightedEntryRaw>,
    #[serde(default)]
    structure_pool: Vec<WeightedEntryRaw>,
}

#[derive(Deserialize)]
struct DungeonsFile {
    active: Option<String>,
    #[serde(default)]
    dungeons: Vec<DungeonRaw>,
}

#[derive(Deserialize)]
struct DungeonRaw {
    id: String,
    #[serde(default)]
    floors: Vec<DungeonFloorRaw>,
}

#[derive(Deserialize)]
struct DungeonFloorRaw {
    min: u32,
    max: u32,
    floor_profile: String,
}

#[derive(Deserialize)]
struct WeightedEntryRaw {
    id: String,
    weight: u32,
}

struct DungeonConfig {
    active_dungeon: String,
    biome_sets: HashMap<String, Vec<u8>>,
    floor_profiles: HashMap<String, FloorProfile>,
    dungeons: HashMap<String, Vec<DungeonFloorRaw>>,
}

#[derive(Clone)]
struct FloorProfile {
    biome_set: String,
    map_pattern: String,
    terrain_theme: String,
    stairs_mode: String,
    special_facility_mode: String,
    enemy_pool: Vec<(String, u32)>,
    drop_pool: Vec<(Item, u32)>,
    structure_pool: Vec<(String, u32)>,
}

fn default_map_pattern() -> String {
    "perlin".to_string()
}

fn default_terrain_theme() -> String {
    "surface_ruin".to_string()
}

fn default_stairs_mode() -> String {
    "normal".to_string()
}

fn default_special_facility_mode() -> String {
    "none".to_string()
}

fn default_biome_ids() -> Vec<u8> {
    (0u8..=15u8).collect()
}

fn sanitize_biomes(raw: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for &b in raw {
        if b <= 15 && !out.contains(&b) {
            out.push(b);
        }
    }
    if out.is_empty() {
        default_biome_ids()
    } else {
        out
    }
}

fn parse_toml<T: for<'de> Deserialize<'de>>(name: &str, src: &str) -> Result<T, String> {
    toml::from_str(src).map_err(|e| format!("failed to parse {name}: {e}"))
}

fn validate_and_build(
    biomes_raw: BiomesFile,
    floors_raw: FloorsFile,
    dungeons_raw: DungeonsFile,
) -> Result<DungeonConfig, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    let mut biome_sets: HashMap<String, Vec<u8>> = HashMap::new();
    for set in biomes_raw.biome_sets {
        let id = set.id.trim().to_string();
        if id.is_empty() {
            errors.push("biome_sets has an entry with empty id".to_string());
            continue;
        }
        if biome_sets.contains_key(&id) {
            errors.push(format!("duplicate biome_set id: '{id}'"));
            continue;
        }
        biome_sets.insert(id, sanitize_biomes(&set.biomes));
    }
    if biome_sets.is_empty() {
        biome_sets.insert("all".to_string(), default_biome_ids());
    }

    let mut floor_profiles: HashMap<String, FloorProfile> = HashMap::new();
    for p in floors_raw.floor_profiles {
        let id = p.id.trim().to_string();
        let biome_set = p.biome_set.trim().to_string();
        let map_pattern = p.map_pattern.trim().to_ascii_lowercase();
        let terrain_theme = p.terrain_theme.trim().to_ascii_lowercase();
        let stairs_mode = p.stairs_mode.trim().to_ascii_lowercase();
        let special_facility_mode = p.special_facility_mode.trim().to_ascii_lowercase();
        if id.is_empty() {
            errors.push("floor_profiles has an entry with empty id".to_string());
            continue;
        }
        if biome_set.is_empty() {
            errors.push(format!("floor_profile '{id}' has empty biome_set"));
            continue;
        }
        if map_pattern != "perlin" && map_pattern != "rogue" {
            errors.push(format!(
                "floor_profile '{id}' has invalid map_pattern '{map_pattern}' (expected 'perlin' or 'rogue')"
            ));
            continue;
        }
        if terrain_theme != "surface_ruin"
            && terrain_theme != "burial_vein"
            && terrain_theme != "research_shaft"
            && terrain_theme != "litany_halls"
        {
            errors.push(format!(
                "floor_profile '{id}' has invalid terrain_theme '{terrain_theme}'"
            ));
            continue;
        }
        if stairs_mode != "normal" && stairs_mode != "facility_locked" {
            errors.push(format!(
                "floor_profile '{id}' has invalid stairs_mode '{stairs_mode}'"
            ));
            continue;
        }
        if special_facility_mode != "none" && special_facility_mode != "substory" {
            errors.push(format!(
                "floor_profile '{id}' has invalid special_facility_mode '{special_facility_mode}'"
            ));
            continue;
        }
        if floor_profiles.contains_key(&id) {
            errors.push(format!("duplicate floor_profile id: '{id}'"));
            continue;
        }
        let mut enemy_pool_by_id: HashMap<String, u32> = HashMap::new();
        for e in p.enemy_pool {
            let enemy_id = e.id.trim().to_string();
            if enemy_id.is_empty() {
                errors.push(format!(
                    "floor_profile '{id}' has enemy_pool entry with empty id"
                ));
                continue;
            }
            if e.weight == 0 {
                errors.push(format!(
                    "floor_profile '{id}' enemy_pool '{enemy_id}' has zero weight"
                ));
                continue;
            }
            match defs().creatures.get(&enemy_id) {
                Some(creature) if creature.faction == Faction::Hostile => {}
                Some(_) => errors.push(format!(
                    "floor_profile '{id}' enemy_pool '{enemy_id}' is not hostile"
                )),
                None => errors.push(format!(
                    "floor_profile '{id}' enemy_pool references unknown creature '{enemy_id}'"
                )),
            }
            let next = enemy_pool_by_id
                .get(&enemy_id)
                .copied()
                .unwrap_or(0)
                .saturating_add(e.weight);
            enemy_pool_by_id.insert(enemy_id, next);
        }
        let enemy_pool: Vec<(String, u32)> = enemy_pool_by_id.into_iter().collect();

        let mut drop_pool_by_id: HashMap<String, u32> = HashMap::new();
        for d in p.drop_pool {
            let item_key = d.id.trim().to_string();
            if item_key.is_empty() {
                errors.push(format!(
                    "floor_profile '{id}' has drop_pool entry with empty id"
                ));
                continue;
            }
            if d.weight == 0 {
                errors.push(format!(
                    "floor_profile '{id}' drop_pool '{item_key}' has zero weight"
                ));
                continue;
            }
            let Some(item) = Item::from_key(&item_key) else {
                errors.push(format!(
                    "floor_profile '{id}' drop_pool references unknown item '{item_key}'"
                ));
                continue;
            };
            let canonical_key = item.key().to_string();
            let next = drop_pool_by_id
                .get(&canonical_key)
                .copied()
                .unwrap_or(0)
                .saturating_add(d.weight);
            drop_pool_by_id.insert(canonical_key, next);
        }
        let drop_pool: Vec<(Item, u32)> = drop_pool_by_id
            .into_iter()
            .filter_map(|(k, w)| Item::from_key(&k).map(|item| (item, w)))
            .collect();

        let mut structure_pool_by_id: HashMap<String, u32> = HashMap::new();
        for s in p.structure_pool {
            let structure_id = s.id.trim().to_string();
            if structure_id.is_empty() {
                errors.push(format!(
                    "floor_profile '{id}' has structure_pool entry with empty id"
                ));
                continue;
            }
            if s.weight == 0 {
                errors.push(format!(
                    "floor_profile '{id}' structure_pool '{structure_id}' has zero weight"
                ));
                continue;
            }
            let next = structure_pool_by_id
                .get(&structure_id)
                .copied()
                .unwrap_or(0)
                .saturating_add(s.weight);
            structure_pool_by_id.insert(structure_id, next);
        }
        let structure_pool: Vec<(String, u32)> = structure_pool_by_id.into_iter().collect();

        floor_profiles.insert(
            id,
            FloorProfile {
                biome_set,
                map_pattern,
                terrain_theme,
                stairs_mode,
                special_facility_mode,
                enemy_pool,
                drop_pool,
                structure_pool,
            },
        );
    }
    if floor_profiles.is_empty() {
        floor_profiles.insert(
            "default".to_string(),
            FloorProfile {
                biome_set: "all".to_string(),
                map_pattern: "perlin".to_string(),
                terrain_theme: "surface_ruin".to_string(),
                stairs_mode: "normal".to_string(),
                special_facility_mode: "none".to_string(),
                enemy_pool: Vec::new(),
                drop_pool: Vec::new(),
                structure_pool: Vec::new(),
            },
        );
    }
    for (profile_id, profile) in &floor_profiles {
        if !biome_sets.contains_key(&profile.biome_set) {
            errors.push(format!(
                "floor_profile '{profile_id}' references unknown biome_set '{}'",
                profile.biome_set
            ));
        }
    }

    let mut dungeons: HashMap<String, Vec<DungeonFloorRaw>> = HashMap::new();
    for d in dungeons_raw.dungeons {
        let id = d.id.trim().to_string();
        if id.is_empty() {
            errors.push("dungeons has an entry with empty id".to_string());
            continue;
        }
        if dungeons.contains_key(&id) {
            errors.push(format!("duplicate dungeon id: '{id}'"));
            continue;
        }
        if d.floors.is_empty() {
            errors.push(format!("dungeon '{id}' has no floors"));
        }
        for floor in &d.floors {
            if floor.min > floor.max {
                errors.push(format!(
                    "dungeon '{id}' has invalid floor range: min {} > max {}",
                    floor.min, floor.max
                ));
            }
            if !floor_profiles.contains_key(&floor.floor_profile) {
                errors.push(format!(
                    "dungeon '{id}' references unknown floor_profile '{}'",
                    floor.floor_profile
                ));
            }
        }
        dungeons.insert(id, d.floors);
    }
    if dungeons.is_empty() {
        dungeons.insert(
            "default".to_string(),
            vec![DungeonFloorRaw {
                min: 1,
                max: u32::MAX,
                floor_profile: "default".to_string(),
            }],
        );
    }

    if let Some(active) = dungeons_raw.active.as_deref() {
        if !active.trim().is_empty() && !dungeons.contains_key(active) {
            errors.push(format!("active dungeon '{active}' is not defined"));
        }
    }

    let active_dungeon = dungeons_raw
        .active
        .filter(|s| dungeons.contains_key(s.as_str()))
        .unwrap_or_else(|| {
            dungeons
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "default".to_string())
        });

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(DungeonConfig {
        active_dungeon,
        biome_sets,
        floor_profiles,
        dungeons,
    })
}

fn load_config() -> DungeonConfig {
    let biomes_raw: BiomesFile =
        parse_toml("data/biomes.toml", include_str!("../data/biomes.toml"))
            .unwrap_or_else(|e| panic!("{e}"));
    let floors_raw: FloorsFile =
        parse_toml("data/floors.toml", include_str!("../data/floors.toml"))
            .unwrap_or_else(|e| panic!("{e}"));
    let dungeons_raw: DungeonsFile =
        parse_toml("data/dungeons.toml", include_str!("../data/dungeons.toml"))
            .unwrap_or_else(|e| panic!("{e}"));

    validate_and_build(biomes_raw, floors_raw, dungeons_raw)
        .unwrap_or_else(|errs| panic!("invalid dungeon configuration:\n- {}", errs.join("\n- ")))
}

#[cfg(test)]
fn load_config_from_strs(
    biomes_toml: &str,
    floors_toml: &str,
    dungeons_toml: &str,
) -> Result<DungeonConfig, Vec<String>> {
    let biomes_raw: BiomesFile = parse_toml("biomes", biomes_toml).map_err(|e| vec![e])?;
    let floors_raw: FloorsFile = parse_toml("floors", floors_toml).map_err(|e| vec![e])?;
    let dungeons_raw: DungeonsFile = parse_toml("dungeons", dungeons_toml).map_err(|e| vec![e])?;
    validate_and_build(biomes_raw, floors_raw, dungeons_raw)
}

#[cfg(test)]
mod tests {
    use super::load_config_from_strs;

    #[test]
    fn validates_invalid_references_and_ranges() {
        let biomes = r#"
[[biome_sets]]
id = "all"
biomes = [0, 1, 2]
"#;
        let floors = r#"
[[floor_profiles]]
id = "f1"
biome_set = "missing_set"
enemy_pool = [{ id = "unknown_enemy", weight = 1 }]
drop_pool = [{ id = "unknown_item", weight = 1 }]
"#;
        let dungeons = r#"
active = "missing_dungeon"

[[dungeons]]
id = "d1"

[[dungeons.floors]]
min = 10
max = 1
floor_profile = "missing_profile"
"#;
        let errs = match load_config_from_strs(biomes, floors, dungeons) {
            Ok(_) => panic!("expected validation error"),
            Err(errs) => errs,
        };
        assert!(
            errs.iter()
                .any(|e| e.contains("unknown biome_set 'missing_set'"))
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("unknown floor_profile 'missing_profile'"))
        );
        assert!(errs.iter().any(|e| e.contains("min 10 > max 1")));
        assert!(
            errs.iter()
                .any(|e| e.contains("active dungeon 'missing_dungeon' is not defined"))
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("enemy_pool references unknown creature 'unknown_enemy'"))
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("drop_pool references unknown item 'unknown_item'"))
        );
    }

    #[test]
    fn validates_duplicate_and_empty_ids() {
        let biomes = r#"
[[biome_sets]]
id = ""
biomes = [0]

[[biome_sets]]
id = "all"
biomes = [0]

[[biome_sets]]
id = "all"
biomes = [1]
"#;
        let floors = r#"
[[floor_profiles]]
id = "default"
biome_set = "all"

[[floor_profiles]]
id = "default"
biome_set = "all"
"#;
        let dungeons = r#"
[[dungeons]]
id = "default"

[[dungeons]]
id = "default"
"#;
        let errs = match load_config_from_strs(biomes, floors, dungeons) {
            Ok(_) => panic!("expected validation error"),
            Err(errs) => errs,
        };
        assert!(errs.iter().any(|e| e.contains("empty id")));
        assert!(errs.iter().any(|e| e.contains("duplicate biome_set id")));
        assert!(
            errs.iter()
                .any(|e| e.contains("duplicate floor_profile id"))
        );
        assert!(errs.iter().any(|e| e.contains("duplicate dungeon id")));
    }

    #[test]
    fn accepts_valid_config() {
        let biomes = r#"
[[biome_sets]]
id = "all"
biomes = [0, 1, 2, 3]
"#;
        let floors = r#"
[[floor_profiles]]
id = "default"
biome_set = "all"
enemy_pool = [{ id = "melted_husk", weight = 100 }, { id = "feral_vessel", weight = 50 }]
drop_pool = [{ id = "potion", weight = 4 }, { id = "herb", weight = 6 }]
"#;
        let dungeons = r#"
active = "default"

[[dungeons]]
id = "default"

[[dungeons.floors]]
min = 1
max = 999
floor_profile = "default"
"#;
        let cfg = load_config_from_strs(biomes, floors, dungeons).unwrap();
        assert_eq!(cfg.active_dungeon, "default");
        let profile = cfg.floor_profiles.get("default").expect("missing profile");
        assert_eq!(profile.enemy_pool.len(), 2);
        assert_eq!(profile.drop_pool.len(), 2);
    }

    #[test]
    fn switches_pool_by_floor_range() {
        let biomes = r#"
[[biome_sets]]
id = "all"
biomes = [0, 1, 2, 3]
"#;
        let floors = r#"
[[floor_profiles]]
id = "shallow"
biome_set = "all"
enemy_pool = [{ id = "melted_husk", weight = 100 }]
drop_pool = [{ id = "potion", weight = 10 }]

[[floor_profiles]]
id = "deep"
biome_set = "all"
enemy_pool = [{ id = "cathedral_frame", weight = 80 }]
drop_pool = [{ id = "elixir", weight = 10 }]
"#;
        let dungeons = r#"
active = "default"

[[dungeons]]
id = "default"

[[dungeons.floors]]
min = 1
max = 3
floor_profile = "shallow"

[[dungeons.floors]]
min = 4
max = 999
floor_profile = "deep"
"#;
        let cfg = load_config_from_strs(biomes, floors, dungeons).unwrap();

        let p1 = super::profile_for_floor(&cfg, 2).expect("missing floor 2 profile");
        let p2 = super::profile_for_floor(&cfg, 7).expect("missing floor 7 profile");
        assert!(p1.enemy_pool.iter().any(|(id, _)| id == "melted_husk"));
        assert!(p2.enemy_pool.iter().any(|(id, _)| id == "cathedral_frame"));
    }
}

fn config() -> &'static DungeonConfig {
    static CFG: OnceLock<DungeonConfig> = OnceLock::new();
    CFG.get_or_init(load_config)
}

fn profile_for_floor(cfg: &DungeonConfig, floor: u32) -> Option<&FloorProfile> {
    let entries = cfg.dungeons.get(&cfg.active_dungeon)?;
    for e in entries {
        if floor >= e.min && floor <= e.max {
            return cfg.floor_profiles.get(&e.floor_profile);
        }
    }
    None
}

pub(crate) fn biomes_for_floor(floor: u32) -> Vec<u8> {
    let cfg = config();
    let Some(profile) = profile_for_floor(cfg, floor) else {
        return default_biome_ids();
    };
    if let Some(biomes) = cfg.biome_sets.get(&profile.biome_set) {
        return biomes.clone();
    }
    default_biome_ids()
}

pub(crate) fn enemy_pool_for_floor(floor: u32) -> Vec<(String, u32)> {
    let cfg = config();
    profile_for_floor(cfg, floor)
        .map(|p| p.enemy_pool.clone())
        .unwrap_or_default()
}

pub(crate) fn drop_pool_for_floor(floor: u32) -> Vec<(Item, u32)> {
    let cfg = config();
    profile_for_floor(cfg, floor)
        .map(|p| p.drop_pool.clone())
        .unwrap_or_default()
}

pub(crate) fn map_pattern_for_floor(floor: u32) -> String {
    let cfg = config();
    profile_for_floor(cfg, floor)
        .map(|p| p.map_pattern.clone())
        .unwrap_or_else(|| "perlin".to_string())
}

pub(crate) fn terrain_theme_for_floor(floor: u32) -> String {
    let cfg = config();
    profile_for_floor(cfg, floor)
        .map(|p| p.terrain_theme.clone())
        .unwrap_or_else(|| "surface_ruin".to_string())
}

pub(crate) fn structure_pool_for_floor(floor: u32) -> Vec<(String, u32)> {
    let cfg = config();
    profile_for_floor(cfg, floor)
        .map(|p| p.structure_pool.clone())
        .unwrap_or_default()
}

pub(crate) fn stairs_mode_for_floor(floor: u32) -> String {
    let cfg = config();
    profile_for_floor(cfg, floor)
        .map(|p| p.stairs_mode.clone())
        .unwrap_or_else(|| "normal".to_string())
}

pub(crate) fn special_facility_mode_for_floor(floor: u32) -> String {
    let cfg = config();
    profile_for_floor(cfg, floor)
        .map(|p| p.special_facility_mode.clone())
        .unwrap_or_else(|| "none".to_string())
}
