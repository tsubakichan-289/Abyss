mod noise;
mod defs;
mod game;
mod save;
mod text;

use std::collections::{HashMap, HashSet};
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
use defs::{RecipeDef, creature_meta, defs, item_meta, tile_meta};
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
const TURN_REGEN_INTERVAL: u64 = 8;
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
    BlinkScroll,
    NovaScroll,
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
            Self::BlinkScroll => "blink_scroll",
            Self::NovaScroll => "nova_scroll",
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
            "blink_scroll" => Some(Self::BlinkScroll),
            "nova_scroll" => Some(Self::NovaScroll),
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
    tr_or_fallback(
        format!("item.name.{}", item.key()),
        &item_meta(item).name,
    )
}

fn localized_item_status(item: Item) -> String {
    tr_or_fallback(
        format!("item.status.{}", item.key()),
        &item_meta(item).status,
    )
}

fn localized_item_description(item: Item) -> String {
    tr_or_fallback(
        format!("item.description.{}", item.key()),
        &item_meta(item).description,
    )
}

pub(crate) fn localized_creature_name(id: &str) -> String {
    tr_or_fallback(
        format!("creature.name.{id}"),
        &creature_meta(id).name,
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct InventoryItem {
    kind: Item,
    custom_name: Option<String>,
    #[serde(default = "default_inventory_qty")]
    qty: u16,
}

fn default_inventory_qty() -> u16 {
    1
}

impl InventoryItem {
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

#[derive(Clone, Debug)]
enum UiMode {
    Normal,
    DebugConsole { input: String },
    StairsPrompt { selected_action: StairsAction },
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
    let side_areas = Layout::vertical([Constraint::Length(16), Constraint::Min(10)]).split(areas[1]);
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
        UiMode::Normal => {}
        UiMode::DebugConsole { input } => {
            let area = centered_rect(70, 28, frame.area());
            frame.render_widget(Clear, area);
            let widget = Paragraph::new(build_debug_console_lines(input))
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
        UiMode::Inventory { selected } => {
            render_inventory_modal(frame, game, *selected);
        }
        UiMode::Settings { selected } => {
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

fn build_map_lines(game: &mut Game, width: u16, height: u16) -> Vec<Line<'static>> {
    let cells_w = (width.saturating_add(1)) / 2;
    let cells_h = (height.saturating_add(1)) / 2;
    let render_w = cells_w.saturating_mul(2).saturating_sub(1);
    let render_h = cells_h.saturating_mul(2).saturating_sub(1);
    let mut lines = Vec::with_capacity(render_h as usize);
    let center_x = (cells_w / 2) as i32;
    let center_y = (cells_h / 2) as i32;

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
            let world_x = game.player.x + dx;
            let world_y = game.player.y + dy;
            let lit_by_torch = game.is_lit_by_torch(world_x, world_y);
            let in_vision = dx * dx + dy * dy <= VISION_RADIUS * VISION_RADIUS;
            let bright = is_bright_by_facing(game.facing, dx, dy);
            let dim_mod = if bright { Modifier::empty() } else { Modifier::DIM };

            let span = if !in_vision && !lit_by_torch {
                Span::raw(" ")
            } else if let Some((glyph, color, bold)) = effect_cells.get(&(world_x, world_y)) {
                let mut style = Style::default().fg(*color).add_modifier(dim_mod);
                if *bold {
                    style = style.bold();
                }
                Span::styled(glyph.to_string(), style)
            } else if effect_targets.contains(&(world_x, world_y)) {
                Span::styled(
                    "*",
                    Style::default().fg(Color::Yellow).bold().add_modifier(dim_mod),
                )
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
            } else if game.has_torch_at(world_x, world_y) {
                let torch_color = if bright {
                    Color::Indexed(220)
                } else {
                    Color::Indexed(94)
                };
                Span::styled("i", Style::default().fg(torch_color).bold().add_modifier(dim_mod))
            } else if game.has_blood_stain(world_x, world_y) {
                Span::styled(
                    "*",
                    Style::default().fg(Color::Indexed(52)).bold(),
                )
            } else {
                let t = game.tile(world_x, world_y);
                let fg = if bright { t.color() } else { shadow_color(t) };
                Span::styled(t.glyph().to_string(), Style::default().fg(fg).add_modifier(dim_mod))
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
        Line::from(trf(
            "status.atk_def",
            &[
                ("atk", game.player_attack_power().to_string()),
                ("def", game.player_defense().to_string()),
            ],
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
        .map(|entry| Line::from(entry.clone()))
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
            if equipped_sword
                .is_some_and(|eq| eq.kind == item.kind && eq.custom_name == item.custom_name)
            {
                equip_badges.push('W');
            }
            if equipped_shield
                .is_some_and(|eq| eq.kind == item.kind && eq.custom_name == item.custom_name)
            {
                equip_badges.push('S');
            }
            if equipped_accessory
                .is_some_and(|eq| eq.kind == item.kind && eq.custom_name == item.custom_name)
            {
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
        if equipped_sword
            .is_some_and(|eq| eq.kind == item.kind && eq.custom_name == item.custom_name)
        {
            equip_badges.push('W');
        }
        if equipped_shield
            .is_some_and(|eq| eq.kind == item.kind && eq.custom_name == item.custom_name)
        {
            equip_badges.push('S');
        }
        if equipped_accessory
            .is_some_and(|eq| eq.kind == item.kind && eq.custom_name == item.custom_name)
        {
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
        lines.push(Line::from(vec![
            Span::styled(if idx == selected { ">" } else { " " }, marker_style),
            Span::raw(" "),
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
    vec![
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
        Line::from(localized_item_description(item.kind)),
    ]
}

fn render_inventory_modal(frame: &mut Frame, game: &Game, selected: usize) {
    let area = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, area);
    let container = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_chrome_color(game)))
        .title(tr("title.inventory"));
    let inner = container.inner(area);
    frame.render_widget(container, area);

    let cols = Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).split(inner);
    let left = Paragraph::new(build_inventory_lines(game, selected))
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

fn build_debug_console_lines(input: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(tr("debug.help.1")),
        Line::from(tr("debug.help.2")),
        Line::from(tr("debug.help.3")),
        Line::from(tr("debug.help.4")),
        Line::from(tr("debug.help.5")),
        Line::raw(""),
        Line::from(format!("{}{}", tr("debug.prompt"), input)),
    ]
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
                game.push_log(tr("debug.give_usage"));
                return;
            };
            let requested = parts
                .next()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(1)
                .max(1);
            let Some(kind) = Item::from_key(item_key) else {
                game.push_log(trf(
                    "debug.give_unknown_item",
                    &[("item", item_key.to_string())],
                ));
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
                game.push_log(trf(
                    "debug.give_ok",
                    &[
                        ("item", localized_item_name(kind)),
                        ("count", added.to_string()),
                    ],
                ));
            } else {
                game.push_log(trf(
                    "debug.give_partial",
                    &[
                        ("item", localized_item_name(kind)),
                        ("added", added.to_string()),
                        ("requested", requested.to_string()),
                    ],
                ));
            }
        }
        "tp" => {
            let Some(arg1) = parts.next() else {
                game.push_log(tr("debug.tp_usage"));
                return;
            };
            if arg1.eq_ignore_ascii_case("exit")
                || arg1.eq_ignore_ascii_case("stairs")
                || arg1 == "出口"
            {
                if let Some(pos) = game.find_nearest_stairs(512) {
                    match game.teleport_player(pos.x, pos.y) {
                        Ok(()) => game.push_log(trf(
                            "debug.tp_ok",
                            &[("x", pos.x.to_string()), ("y", pos.y.to_string())],
                        )),
                        Err(msg) => game.push_log(msg),
                    }
                } else {
                    game.push_log(tr("debug.exit_not_found"));
                }
                return;
            }
            let Some(arg2) = parts.next() else {
                game.push_log(tr("debug.tp_usage"));
                return;
            };
            let Ok(x) = arg1.parse::<i32>() else {
                game.push_log(tr("debug.tp_usage"));
                return;
            };
            let Ok(y) = arg2.parse::<i32>() else {
                game.push_log(tr("debug.tp_usage"));
                return;
            };
            match game.teleport_player(x, y) {
                Ok(()) => game.push_log(trf(
                    "debug.tp_ok",
                    &[("x", x.to_string()), ("y", y.to_string())],
                )),
                Err(msg) => game.push_log(msg),
            }
        }
        "find_exit" | "exit_search" | "出口探索" => {
            if let Some(pos) = game.find_nearest_stairs(512) {
                game.push_log(trf(
                    "debug.exit_found",
                    &[("x", pos.x.to_string()), ("y", pos.y.to_string())],
                ));
            } else {
                game.push_log(tr("debug.exit_not_found"));
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
                    game.push_log(tr("debug.inv_usage"));
                    return;
                }
            }
            game.push_log(if game.invincible() {
                tr("debug.inv_on")
            } else {
                tr("debug.inv_off")
            });
        }
        _ => {
            game.push_log(trf("debug.unknown", &[("cmd", trimmed.to_string())]));
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
        let localized_name = tr_or_fallback(
            format!("lang.name.{code}"),
            name,
        );
        lines.push(Line::from(format!("{marker} {localized_name} ({code}) {current}")));
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

fn build_dead_summary_lines(game: &Game) -> Vec<Line<'static>> {
    vec![
        Line::from(tr("death.header")),
        Line::from(tr("death.help")),
        Line::raw(""),
        Line::from(trf("death.cause", &[("v", game.death_cause_text())])),
        Line::from(trf("death.stat.turn", &[("v", game.turn.to_string())])),
        Line::from(trf("death.stat.level", &[("v", game.level.to_string())])),
        Line::from(trf("death.stat.exp_total", &[("v", game.stat_total_exp.to_string())])),
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
    lines.extend(game.logs[start..].iter().cloned().map(Line::from));
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
    if let Err(e) = run(debug_enabled) {
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
        let Some(picked) = game.take_inventory_one(selected) else {
            game.push_log(tr("craft.log.no_inv_selected"));
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

fn transition_to_dead(game: &mut Game, ui_mode: &mut UiMode, death_processed: &mut bool) {
    if !*death_processed {
        erase_save_on_death(game);
        *death_processed = true;
    }
    *ui_mode = UiMode::Dead {
        scroll: 0,
        selected_action: DeadAction::Restart,
    };
}

fn run(debug_enabled: bool) -> io::Result<()> {
    let _guard = TerminalGuard::new()?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut game = load_game_or_new(initial_seed());
    let mut esc_hold_count: u8 = 0;
    let mut ui_mode = UiMode::Normal;
    let mut death_processed = false;

    loop {
        terminal.draw(|frame| render_ui(frame, &mut game, esc_hold_count, &ui_mode))?;
        game.advance_effects();

        if game.player_hp <= 0 && !matches!(ui_mode, UiMode::Dead { .. }) {
            transition_to_dead(&mut game, &mut ui_mode, &mut death_processed);
            continue;
        }

        if game.has_pending_effects() {
            while event::poll(Duration::from_millis(0))? {
                let _ = event::read()?;
            }
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

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

                    if debug_enabled && matches!(key.code, KeyCode::Char('/')) {
                        ui_mode = UiMode::DebugConsole {
                            input: String::new(),
                        };
                        continue;
                    }

                    if matches!(
                        key.code,
                        KeyCode::Char('v') | KeyCode::Char('V') | KeyCode::Char('m') | KeyCode::Char('M')
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
                                if game.use_inventory_item(idx) {
                                    game.consume_non_attack_turn();
                                    if game.player_hp <= 0 {
                                        transition_to_dead(
                                            &mut game,
                                            &mut ui_mode,
                                            &mut death_processed,
                                        );
                                    } else {
                                        persist_game(&mut game);
                                    }
                                }
                            }
                            continue;
                        }
                    }

                    let action = if let Some((dx, dy)) = movement_delta(key.code) {
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
                        let before = game.player;
                        let moved = matches!(action, Action::Move(_, _));
                        game.apply_action(action);
                        if game.player_hp <= 0 {
                            transition_to_dead(&mut game, &mut ui_mode, &mut death_processed);
                        } else {
                            let stepped_on_stairs = moved
                                && (game.player.x != before.x || game.player.y != before.y)
                                && game.is_on_stairs();
                            if stepped_on_stairs {
                                ui_mode = UiMode::StairsPrompt {
                                    selected_action: StairsAction::Descend,
                                };
                            } else if let Some(text) = game.take_pending_dialogue() {
                                game.push_log(text);
                            }
                            persist_game(&mut game);
                        }
                    }
                }
                UiMode::DebugConsole { input } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc => {
                            ui_mode = UiMode::Normal;
                        }
                        KeyCode::Enter => {
                            let cmd = input.trim().to_string();
                            if !cmd.is_empty() {
                                execute_debug_command(&mut game, &cmd);
                                persist_game(&mut game);
                            }
                            ui_mode = UiMode::Normal;
                        }
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        KeyCode::Char(ch) => {
                            if !ch.is_control() && input.len() < 128 {
                                input.push(ch);
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
                                persist_game(&mut game);
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
                        }
                        }
                        _ => {}
                    }
                }
                UiMode::Settings { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let langs = available_languages();
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::MainMenu { selected: 0 }
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
                    if matches!(key.code, KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V')) {
                        ui_mode = UiMode::MainMenu { selected: 0 };
                    }
                }
                UiMode::Inventory { selected } => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let len = game.inventory_len();
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::MainMenu { selected: 0 }
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            if len > 0 {
                                *selected = selected.saturating_sub(1);
                            }
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            if len > 0 {
                                *selected = (*selected + 1).min(len - 1);
                            }
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
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
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::Inventory {
                                selected: *selected,
                            };
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            *action_idx = action_idx.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                            *action_idx = (*action_idx + 1).min(ITEM_MENU_ACTIONS.len() - 1);
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
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
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                            );
                                        } else {
                                            persist_game(&mut game);
                                        }
                                    }
                                    if game.player_hp <= 0 {
                                        continue;
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
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                            );
                                        } else {
                                            persist_game(&mut game);
                                        }
                                    }
                                    if game.player_hp <= 0 {
                                        continue;
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
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                            );
                                        } else {
                                            persist_game(&mut game);
                                        }
                                    }
                                    if game.player_hp <= 0 {
                                        continue;
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
                                transition_to_dead(&mut game, &mut ui_mode, &mut death_processed);
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
                        KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => {
                            ui_mode = UiMode::ItemMenu {
                                selected: item_idx,
                                action_idx: 0,
                            };
                        }
                        KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
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
                        KeyCode::Esc
                        | KeyCode::Tab
                        | KeyCode::Char('v')
                        | KeyCode::Char('V') => {
                            if *focus == CraftFocus::Inventory {
                                *focus = CraftFocus::Grid;
                            } else {
                                close_crafting_mode(&mut game, grid);
                                ui_mode = UiMode::MainMenu { selected: 1 };
                            }
                        }
                        KeyCode::Up => {
                            match *focus {
                                CraftFocus::Grid => {
                                    *cursor = move_cursor_3x3(*cursor, 0, -1);
                                }
                                CraftFocus::Inventory | CraftFocus::CraftButton => {
                                    *focus = CraftFocus::Grid;
                                }
                            }
                        }
                        KeyCode::Down => {
                            match *focus {
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
                            }
                        }
                        KeyCode::Left => {
                            match *focus {
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
                            }
                        }
                        KeyCode::Right => {
                            match *focus {
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
                            }
                        }
                        KeyCode::Char(ch) => match ch.to_ascii_lowercase() {
                            'w' => {
                                match *focus {
                                    CraftFocus::Grid => {
                                        *cursor = move_cursor_3x3(*cursor, 0, -1);
                                    }
                                    CraftFocus::Inventory | CraftFocus::CraftButton => {
                                        *focus = CraftFocus::Grid;
                                    }
                                }
                            }
                            's' => {
                                match *focus {
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
                                }
                            }
                            'a' => {
                                match *focus {
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
                                }
                            }
                            'd' => {
                                match *focus {
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
                                }
                            }
                            'f' | ' ' => {
                                match *focus {
                                    CraftFocus::Grid => {
                                        let idx = *cursor;
                                        if let Some(item) = grid[idx].take() {
                                            game.stash_or_drop_item(item);
                                        } else if inv_len > 0 {
                                            *focus = CraftFocus::Inventory;
                                        } else {
                                            game.push_log(tr("log.no_inv_to_place"));
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
                                        if execute_crafting(&mut game, grid) {
                                            game.consume_non_attack_turn();
                                            if game.player_hp <= 0 {
                                                transition_to_dead(
                                                    &mut game,
                                                    &mut ui_mode,
                                                    &mut death_processed,
                                                );
                                            } else {
                                                persist_game(&mut game);
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        },
                        KeyCode::Enter => {
                            match *focus {
                                CraftFocus::Grid => {
                                    let idx = *cursor;
                                    if let Some(item) = grid[idx].take() {
                                        game.stash_or_drop_item(item);
                                    } else if inv_len > 0 {
                                        *focus = CraftFocus::Inventory;
                                    } else {
                                        game.push_log(tr("log.no_inv_to_place"));
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
                                    if execute_crafting(&mut game, grid) {
                                        game.consume_non_attack_turn();
                                        if game.player_hp <= 0 {
                                            transition_to_dead(
                                                &mut game,
                                                &mut ui_mode,
                                                &mut death_processed,
                                            );
                                        } else {
                                            persist_game(&mut game);
                                        }
                                    }
                                }
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
