mod noise;
mod defs;
mod game;
mod save;
mod text;

use std::collections::HashMap;
use std::env;
use std::io;
use std::path::Path;
use std::time::SystemTime;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use defs::{Faction, RecipeDef, creature_meta, defs, item_meta, tile_meta};
use game::{Action, Game};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
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
const MAX_INVENTORY: usize = 10;
const SAVE_FILE: &str = "savegame.json";

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Item {
    Potion,
    Herb,
    Elixir,
    Wood,
    Stone,
    IronIngot,
    Hide,
    StringFiber,
    StoneAxe,
    IronSword,
    IronPickaxe,
}

impl Item {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Potion => "potion",
            Self::Herb => "herb",
            Self::Elixir => "elixir",
            Self::Wood => "wood",
            Self::Stone => "stone",
            Self::IronIngot => "iron_ingot",
            Self::Hide => "hide",
            Self::StringFiber => "string",
            Self::StoneAxe => "stone_axe",
            Self::IronSword => "iron_sword",
            Self::IronPickaxe => "iron_pickaxe",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "potion" => Some(Self::Potion),
            "herb" => Some(Self::Herb),
            "elixir" => Some(Self::Elixir),
            "wood" => Some(Self::Wood),
            "stone" => Some(Self::Stone),
            "iron_ingot" => Some(Self::IronIngot),
            "hide" => Some(Self::Hide),
            "string" => Some(Self::StringFiber),
            "stone_axe" => Some(Self::StoneAxe),
            "iron_sword" => Some(Self::IronSword),
            "iron_pickaxe" => Some(Self::IronPickaxe),
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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

fn biome_tint_color(base: Color, biome: u8, bright: bool) -> Color {
    let bx = biome % 4;
    let by = biome / 4;
    if !bright {
        return match (bx, by) {
            (0, 0) | (1, 0) | (0, 1) => Color::Indexed(18),
            (2, 1) | (1, 2) | (2, 2) => Color::Indexed(22),
            (3, 2) | (2, 3) | (3, 3) => Color::Indexed(240),
            _ => Color::Indexed(94),
        };
    }

    match (bx, by) {
        // wet / lowland
        (0, 0) | (1, 0) | (0, 1) => match base {
            Color::Indexed(70) => Color::Indexed(78),
            Color::Indexed(28) => Color::Indexed(30),
            _ => base,
        },
        // lush
        (2, 1) | (1, 2) | (2, 2) => match base {
            Color::Indexed(70) => Color::Indexed(82),
            Color::Indexed(28) => Color::Indexed(34),
            _ => base,
        },
        // rocky / high
        (3, 2) | (2, 3) | (3, 3) => match base {
            Color::Indexed(70) => Color::Indexed(143),
            Color::Indexed(28) => Color::Indexed(101),
            Color::Indexed(245) => Color::Indexed(252),
            _ => base,
        },
        _ => base,
    }
}

fn biome_tile_glyph(tile: Tile, biome: u8) -> char {
    let bx = biome % 4;
    let by = biome / 4;
    match tile {
        Tile::Grass => match (bx, by) {
            (0, 0) | (1, 0) => ',',
            (3, 2) | (2, 3) | (3, 3) => ';',
            _ => '"',
        },
        Tile::Forest => match (bx, by) {
            (0, 0) | (1, 0) => 'Y',
            (3, 2) | (2, 3) | (3, 3) => 'A',
            _ => 'T',
        },
        Tile::Sand => match (bx, by) {
            (0, 0) | (1, 0) => ':',
            _ => '.',
        },
        Tile::Rock => match (bx, by) {
            (3, 2) | (2, 3) | (3, 3) => 'O',
            _ => 'o',
        },
        _ => tile.glyph(),
    }
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

#[derive(Clone, Debug)]
enum UiMode {
    Normal,
    MainMenu { selected: usize },
    Inventory { selected: usize },
    ItemMenu { selected: usize, action_idx: usize },
    RenameItem { selected: usize, input: String },
    Settings { selected: usize },
    Hints,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainMenuEntry {
    Items,
    Crafting,
    Settings,
    Hints,
    Exit,
}

impl MainMenuEntry {
    fn label(self) -> &'static str {
        match self {
            Self::Items => tr("main_menu.items"),
            Self::Crafting => tr("main_menu.crafting"),
            Self::Settings => tr("main_menu.settings"),
            Self::Hints => tr("main_menu.hints"),
            Self::Exit => tr("main_menu.exit"),
        }
    }
}

const MAIN_MENU_ENTRIES: [MainMenuEntry; 5] = [
    MainMenuEntry::Items,
    MainMenuEntry::Crafting,
    MainMenuEntry::Settings,
    MainMenuEntry::Hints,
    MainMenuEntry::Exit,
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
    let areas = Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(frame.area());
    let side_areas = Layout::vertical([
        Constraint::Length(14),
        Constraint::Min(10),
        Constraint::Length(5),
    ])
    .split(areas[1]);

    let map_block = Block::default().borders(Borders::ALL).title(tr("title.map"));
    let map_inner = map_block.inner(areas[0]);
    let map_lines = build_map_lines(game, map_inner.width, map_inner.height);
    let map_widget = Paragraph::new(map_lines).block(map_block);
    frame.render_widget(map_widget, areas[0]);

    let status_widget = Paragraph::new(build_status_lines(game, esc_hold_count))
        .block(Block::default().borders(Borders::ALL).title(tr("title.status")))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(status_widget, side_areas[0]);

    let legend_widget = Paragraph::new(build_legend_lines())
        .block(Block::default().borders(Borders::ALL).title(tr("title.legend")))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(legend_widget, side_areas[1]);

    let log_height = side_areas[2].height.saturating_sub(2) as usize;
    let log_widget = Paragraph::new(build_log_lines(game, log_height))
        .block(Block::default().borders(Borders::ALL).title(tr("title.log")))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(log_widget, side_areas[2]);

    match ui_mode {
        UiMode::Normal => {}
        UiMode::MainMenu { selected } => {
            let area = centered_rect(40, 45, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_main_menu_lines(*selected))
                .block(Block::default().borders(Borders::ALL).title(tr("title.menu")))
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::Inventory { selected } => {
            render_inventory_modal(frame, game, *selected);
        }
        UiMode::Settings { selected } => {
            let area = centered_rect(50, 40, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_settings_lines(*selected))
                .block(Block::default().borders(Borders::ALL).title(tr("title.settings")))
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::Hints => {
            let area = centered_rect(55, 55, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_hints_lines())
                .block(Block::default().borders(Borders::ALL).title(tr("title.hints")))
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
                .block(Block::default().borders(Borders::ALL).title(tr("title.item_menu")))
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(widget, area);
        }
        UiMode::RenameItem { selected, input } => {
            let area = centered_rect(55, 30, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_rename_lines(game, *selected, input))
                .block(Block::default().borders(Borders::ALL).title(tr("title.rename_item")))
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
    let cells_w = (width.saturating_add(1)) / 2;
    let cells_h = (height.saturating_add(1)) / 2;
    let render_w = cells_w.saturating_mul(2).saturating_sub(1);
    let render_h = cells_h.saturating_mul(2).saturating_sub(1);
    let mut lines = Vec::with_capacity(render_h as usize);
    let center_x = (cells_w / 2) as i32;
    let center_y = (cells_h / 2) as i32;

    let mut effect_gaps: HashMap<(u16, u16), char> = HashMap::new();
    for fx in &game.attack_effects {
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
            let world_x = game.player.x + dx;
            let world_y = game.player.y + dy;
            let bright = is_bright_by_facing(game.facing, dx, dy);
            let dim_mod = if bright { Modifier::empty() } else { Modifier::DIM };

            let span = if dx * dx + dy * dy > VISION_RADIUS * VISION_RADIUS {
                Span::raw(" ")
            } else if sx as i32 == center_x && sy as i32 == center_y {
                Span::styled("@", Style::default().fg(Color::Red).bold())
            } else if let Some((eglyph, ecolor)) = game.enemy_visual_at(world_x, world_y) {
                let enemy_color = if bright { ecolor } else { Color::Indexed(52) };
                Span::styled(
                    eglyph.to_string(),
                    Style::default().fg(enemy_color).add_modifier(dim_mod),
                )
            } else if let Some(item) = game.item_at(world_x, world_y) {
                let item_color = if bright { item.color() } else { Color::Indexed(94) };
                Span::styled(
                    item.glyph().to_string(),
                    Style::default().fg(item_color).add_modifier(dim_mod),
                )
            } else {
                let t = game.tile(world_x, world_y);
                let biome = game.biome_index_at(world_x, world_y);
                let base_fg = if bright { t.color() } else { shadow_color(t) };
                let fg = biome_tint_color(base_fg, biome, bright);
                let glyph = biome_tile_glyph(t, biome);
                Span::styled(glyph.to_string(), Style::default().fg(fg).add_modifier(dim_mod))
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

    let equipped = game
        .equipped_tool
        .as_ref()
        .map(InventoryItem::display_name)
        .unwrap_or_else(|| tr("status.none").to_string());

    vec![
        Line::from(trf(
            "status.hp",
            &[
                ("hp", game.player_hp.max(0).to_string()),
                ("max", game.player_max_hp.to_string()),
            ],
        )),
        Line::from(trf(
            "status.atk_def",
            &[
                ("atk", game.player_attack_power().to_string()),
                ("def", game.player_defense().to_string()),
            ],
        )),
        Line::from(trf("status.turn", &[("turn", game.turn.to_string())])),
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
        Line::from(trf("status.equipped", &[("name", equipped)])),
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
            &[
                ("label", game.facing.label().to_string()),
                ("glyph", game.facing.glyph().to_string()),
            ],
        )),
        Line::from(trf(
            "status.chunks",
            &[("count", game.generated_chunks().to_string())],
        )),
        Line::from(trf(
            "status.biome",
            &[("name", game.current_biome_name().to_string())],
        )),
        Line::from(trf(
            "status.lang",
            &[("lang", current_lang().to_string())],
        )),
        Line::raw(""),
        Line::from(tr("status.ctrl.wasd")),
        Line::from(tr("status.ctrl.diag")),
        Line::from(tr("status.ctrl.arrows")),
        Line::from(tr("status.ctrl.face")),
        Line::from(tr("status.ctrl.attack")),
        Line::from(tr("status.ctrl.menu")),
        Line::from(tr("status.ctrl.wait")),
        Line::from(esc_line),
    ]
}

fn build_legend_lines() -> Vec<Line<'static>> {
    let player_name = creature_meta("player").name.clone();
    let mut lines = vec![Line::from(format!("@ : {}", player_name))];
    let mut creature_ids: Vec<String> = defs()
        .creatures
        .keys()
        .filter(|id| id.as_str() != "player")
        .cloned()
        .collect();
    creature_ids.sort();
    for id in creature_ids {
        let c = creature_meta(&id);
        let faction = match c.faction {
            Faction::Ally => tr("faction.ally"),
            Faction::Hostile => tr("faction.hostile"),
            Faction::Neutral => tr("faction.neutral"),
        };
        lines.push(Line::from(format!("{} : {} ({})", c.glyph, c.name, faction)));
    }
    let mut item_ids: Vec<String> = defs().items.keys().cloned().collect();
    item_ids.sort();
    for id in item_ids {
        if let Some(item) = Item::from_key(&id) {
            lines.push(Line::from(format!(
                "{} : {}",
                item.glyph(),
                item_meta(item).legend
            )));
        }
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
        Line::from(trf(
            "inventory.header",
            &[
                ("count", game.inventory_len().to_string()),
                ("max", MAX_INVENTORY.to_string()),
            ],
        )),
        Line::from(tr("inventory.help")),
        Line::raw(""),
    ];
    if game.inventory.is_empty() {
        lines.push(Line::from(tr("inventory.empty")));
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

fn build_inventory_detail_lines(game: &Game, selected: usize) -> Vec<Line<'static>> {
    let Some(item) = game.inventory.get(selected) else {
        return vec![
            Line::from(tr("inventory.no_selected")),
            Line::raw(""),
            Line::from(tr("inventory.no_selected_help")),
        ];
    };
    let meta = item_meta(item.kind);
    vec![
        Line::from(vec![
            Span::styled(
                item.kind.glyph().to_string(),
                Style::default().fg(item.kind.color()).bold(),
            ),
            Span::raw(" "),
            Span::styled(item.display_name(), Style::default().bold()),
        ]),
        Line::raw(""),
        Line::from(trf("inventory.type", &[("type", meta.status.clone())])),
        Line::from(trf("inventory.id", &[("id", item.kind.key().to_string())])),
        Line::raw(""),
        Line::from(tr("inventory.description")),
        Line::from(meta.description.clone()),
    ]
}

fn render_inventory_modal(frame: &mut Frame, game: &Game, selected: usize) {
    let area = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, area);
    let container = Block::default()
        .borders(Borders::ALL)
        .title(tr("title.inventory"));
    let inner = container.inner(area);
    frame.render_widget(container, area);

    let cols = Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).split(inner);
    let left = Paragraph::new(build_inventory_lines(game, selected))
        .block(Block::default().borders(Borders::RIGHT))
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
        lines.push(Line::from(format!("{marker} {name} ({code}) {current}")));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(tr("settings.back")));
    lines
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
        game.push_log(tr("craft.log.no_recipe"));
        return false;
    };
    let result = recipe.result;

    for slot in grid.iter_mut() {
        *slot = None;
    }

    if game.add_item_kind_to_inventory(result) {
        game.push_log(trf("craft.log.crafted", &[("label", recipe.label.clone())]));
    } else if game.place_ground_item_near_player(result) {
        game.push_log(trf(
            "craft.log.crafted_drop",
            &[("label", recipe.label.clone())],
        ));
    } else {
        game.push_log(trf(
            "craft.log.crafted_lost",
            &[("label", recipe.label.clone())],
        ));
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
        game.push_log(tr("craft.log.no_inv_selected"));
    }
}

fn load_game_or_new(seed: u64) -> Game {
    let path = Path::new(SAVE_FILE);
    match save::load_game(path) {
        Ok(mut game) => {
            game.push_log(tr("save.loaded"));
            game
        }
        Err(_) => Game::new(seed),
    }
}

fn persist_game(game: &mut Game) {
    let path = Path::new(SAVE_FILE);
    if let Err(err) = save::save_game(path, game) {
        game.push_log(trf("save.failed", &[("error", err)]));
    }
}

fn erase_save_on_death(game: &mut Game) {
    let path = Path::new(SAVE_FILE);
    if save::delete_save(path).is_ok() {
        game.push_log(tr("save.deleted_on_death"));
    } else {
        game.push_log(tr("save.delete_failed_on_death"));
    }
}

fn run() -> io::Result<()> {
    let _guard = TerminalGuard::new()?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut game = load_game_or_new(initial_seed());
    let mut esc_hold_count: u8 = 0;
    let mut ui_mode = UiMode::Normal;

    loop {
        terminal.draw(|frame| render_ui(frame, &mut game, esc_hold_count, &ui_mode))?;
        game.advance_effects();

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
                            game.push_log(trf(
                                "log.hold_esc",
                                &[
                                    ("count", esc_hold_count.to_string()),
                                    ("max", ESC_HOLD_STEPS.to_string()),
                                ],
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

                    if matches!(key.code, KeyCode::Char('m') | KeyCode::Char('M')) {
                        ui_mode = UiMode::MainMenu { selected: 0 };
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
                            erase_save_on_death(&mut game);
                            break;
                        }
                        persist_game(&mut game);
                    }
                }
                UiMode::MainMenu { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc => ui_mode = UiMode::Normal,
                        KeyCode::Up => *selected = selected.saturating_sub(1),
                        KeyCode::Down => {
                            *selected = (*selected + 1).min(MAIN_MENU_ENTRIES.len() - 1);
                        }
                        KeyCode::Enter => match MAIN_MENU_ENTRIES[*selected] {
                            MainMenuEntry::Items => ui_mode = UiMode::Inventory { selected: 0 },
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
                                ui_mode = UiMode::Settings { selected };
                            }
                            MainMenuEntry::Hints => ui_mode = UiMode::Hints,
                            MainMenuEntry::Exit => break,
                        },
                        _ => {}
                    }
                }
                UiMode::Settings { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let langs = available_languages();
                    match key.code {
                        KeyCode::Esc => ui_mode = UiMode::MainMenu { selected: 0 },
                        KeyCode::Up => {
                            *selected = selected.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            if !langs.is_empty() {
                                *selected = (*selected + 1).min(langs.len() - 1);
                            }
                        }
                        KeyCode::Enter => {
                            if let Some((code, name)) = langs.get(*selected) {
                                if set_lang(code) {
                                    game.push_log(trf(
                                        "settings.lang_changed",
                                        &[("name", (*name).to_string())],
                                    ));
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
                    if key.code == KeyCode::Esc {
                        ui_mode = UiMode::MainMenu { selected: 0 };
                    }
                }
                UiMode::Inventory { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let len = game.inventory_len();
                    match key.code {
                        KeyCode::Esc => ui_mode = UiMode::MainMenu { selected: 0 },
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
                                        if game.player_hp <= 0 {
                                            erase_save_on_death(&mut game);
                                            break;
                                        }
                                        persist_game(&mut game);
                                    }
                                    let next_len = game.inventory_len();
                                    if next_len == 0 {
                                        ui_mode = UiMode::Inventory { selected: 0 };
                                    } else {
                                        ui_mode = UiMode::Inventory {
                                            selected: item_idx.min(next_len - 1),
                                        };
                                    }
                                }
                                ItemMenuAction::Throw => {
                                    if game.throw_inventory_item(item_idx) {
                                        game.consume_non_attack_turn();
                                        if game.player_hp <= 0 {
                                            erase_save_on_death(&mut game);
                                            break;
                                        }
                                        persist_game(&mut game);
                                    }
                                    let next_len = game.inventory_len();
                                    if next_len == 0 {
                                        ui_mode = UiMode::Inventory { selected: 0 };
                                    } else {
                                        ui_mode = UiMode::Inventory {
                                            selected: item_idx.min(next_len - 1),
                                        };
                                    }
                                }
                                ItemMenuAction::Use => {
                                    if game.use_inventory_item(item_idx) {
                                        game.consume_non_attack_turn();
                                        if game.player_hp <= 0 {
                                            erase_save_on_death(&mut game);
                                            break;
                                        }
                                        persist_game(&mut game);
                                    }
                                    let next_len = game.inventory_len();
                                    if next_len == 0 {
                                        ui_mode = UiMode::Inventory { selected: 0 };
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
                        ui_mode = UiMode::Inventory { selected: 0 };
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
                            persist_game(&mut game);
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
                                ui_mode = UiMode::MainMenu { selected: 1 };
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
                                        erase_save_on_death(&mut game);
                                        break;
                                    }
                                    persist_game(&mut game);
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
                                        game.push_log(tr("log.no_inv_to_place"));
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
                                    game.push_log(tr("log.no_inv_to_place"));
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
