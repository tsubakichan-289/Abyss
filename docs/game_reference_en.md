# Abyss: Reference (Current Build)

Sources:
- `data/game_data.toml`
- `src/game.rs`
- `src/main.rs`

## Core Controls

- Move: `W/A/S/D` (diagonal: `Q/E/Z/C`)
- Attack / Confirm: `F`
- Wait: `Space`
- Menu / Back: `V`
- Quickbar use: `1..9,0`
- Debug command pane: `/` (debug launch only)

Notes:
- Input is blocked while effects are playing.
- Attack visuals/logs are delayed; enemy attacks are processed in ascending entity ID order.

## Progression Systems

- Turn-based. Failed movement (`Blocked`, etc.) does not consume a turn.
- Hunger decreases by `1` every `3` turns.
- Moving to the next floor (stairs -> descend) fully restores `MP`.
- Level-up grants `+2 max MP` (and restores MP accordingly).
- One down-stairs tile is placed roughly `100` tiles from spawn.

## Vision and Lighting

- Player vision radius: `7`.
- Placed torches light radius `5`.
- Torch-lit tiles remain visible even outside current player vision.
- Dark areas can spawn enemies over time.

## Debug Mode

- Launch args: `--debug` or `-d`
- Debug command pane (`/`) is available only in debug mode.

Available commands:
- `tp x y`
- `tp exit`
- `find_exit` / `exit_search` / `出口探索`
- `give <item_key> [count]`
- `inv [on|off|toggle]` (invincible mode)

## Item List

Notes:
- Inventory size: `10` slots.
- Only material items stack (up to `x10`).
- Scrolls are reusable (not consumed on use).

| Key | Name | Type | Effect / Usage | Stack |
| --- | --- | --- | --- | --- |
| `potion` | Potion | Consumable | HP `+6` | No |
| `herb` | Herb | Consumable | HP `+3` | No |
| `elixir` | Elixir | Consumable | HP `+12` | No |
| `jerky` | Jerky | Consumable | Hunger `+30` | No |
| `bread` | Bread | Consumable | Hunger `+20` | No |
| `torch` | Torch | Tool | Place to light nearby tiles. Can be broken/picked up by attacking | No |
| `flame_scroll` | Flame Scroll | Magic | MP `2`. Forward flame up to 6 tiles. Can be cast with no target | No |
| `blink_scroll` | Blink Scroll | Magic | MP `3`. Blink forward up to 4 tiles | No |
| `nova_scroll` | Nova Scroll | Magic | MP `5`. Damage all targets within radius 4 | No |
| `wood` | Wood | Material | Crafting material | Yes |
| `stone` | Stone | Material | Crafting material | Yes |
| `iron_ingot` | Iron Ingot | Material | Crafting material | Yes |
| `hide` | Hide | Material | Crafting material | Yes |
| `string` | String | Material | Crafting material | Yes |
| `stone_axe` | Stone Axe | Tool/Weapon | Equippable, attack `+2` | No |
| `iron_sword` | Iron Sword | Weapon | Equippable, attack `+4` | No |
| `iron_pickaxe` | Iron Pickaxe | Tool/Weapon | Equippable, attack `+3` | No |
| `wooden_shield` | Wooden Shield | Shield | Equippable, defense `+3` | No |
| `lucky_charm` | Lucky Charm | Accessory | Equippable, attack `+1` / defense `+1` | No |

## Crafting Recipes (3x3)

- Empty slot is shown as `.`.
- Stone Axe has two mirrored recipes.

| Result | Layout (3 rows) |
| --- | --- |
| `stone_axe` | `stone stone .` / `stone wood .` / `. string .` |
| `stone_axe` (mirror) | `. stone stone` / `. wood stone` / `. string .` |
| `elixir` | `. herb .` / `herb potion herb` / `. herb .` |
| `iron_sword` | `. iron_ingot .` / `. iron_ingot .` / `. wood .` |
| `iron_pickaxe` | `iron_ingot iron_ingot iron_ingot` / `. wood .` / `. wood .` |
| `wooden_shield` | `wood wood wood` / `wood hide wood` / `. hide .` |
| `lucky_charm` | `. string .` / `string iron_ingot string` / `. hide .` |

## Enemies and NPCs

Notes:
- Action order is ascending entity ID.
- Hostiles do not chase outside player vision; in that state they patrol via straight movement + clockwise turn on dead ends.
- Inside vision, hostiles pathfind around obstacles.
- Same monster ID always has fixed stats.

| ID | Name | Faction | HP | ATK | DEF | Spawn Weight | First Floor |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| `slime` | Slime | hostile | 2 | 2 | 0 | 100 | 1 |
| `wolf` | Wolf | hostile | 4 | 3 | 1 | 70 | 1 |
| `bat` | Bat | hostile | 3 | 2 | 0 | 60 | 1 |
| `golem` | Golem | hostile | 8 | 4 | 2 | 35 | 1 |
| `slime_brute` | Brute Slime | hostile | 6 | 3 | 1 | 55 | 3 |
| `wolf_alpha` | Alpha Wolf | hostile | 8 | 5 | 2 | 40 | 4 |
| `bat_night` | Night Bat | hostile | 7 | 4 | 1 | 35 | 5 |
| `golem_elder` | Elder Golem | hostile | 14 | 7 | 4 | 22 | 6 |
| `traveler` | Traveler | neutral | 6 | 1 | 1 | 0 | fixed spawn |

Traveler behavior:
- Moves once every 2 turns until attacked.
- After being attacked, enters flee mode and moves away from player.
- Bump-talk before attacked uses normal line; while fleeing it uses flee line.
- Death cry log is for neutral NPCs only.
- Drops `bread` with `40%` chance on defeat.

## Drops

Hostile enemy kill drop:
- Drop chance: `60%`

| Roll Range | Item |
| --- | --- |
| `0..29` | `potion` |
| `30..59` | `herb` |
| `60..73` | `hide` |
| `74..85` | `iron_ingot` |
| `86..92` | `flame_scroll` |
| `93..95` | `blink_scroll` |
| `96..98` | `nova_scroll` |
| `99` | `elixir` |
