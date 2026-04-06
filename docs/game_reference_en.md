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
- Auto-run straight: `Ctrl + Move` (stops at dead end / hostile adjacency / item tile)

Notes:
- Input is blocked while effects are playing.
- Attack visuals/logs are delayed; enemy attacks are processed in ascending entity ID order.
- `Ctrl + Move` updates/render per tile step (not instant teleport).

## Progression Systems

- Turn-based. Failed movement (`Blocked`, etc.) does not consume a turn.
- Hunger decreases by `1` every `4` turns.
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
- Inventory size: `20` slots.
- Only material items stack (up to `x10`).
- Scrolls are reusable (not consumed on use).
- Using `potion` / `herb` / `elixir` also restores hunger by `+5` (scrolls do not).

| Key | Name | Type | Effect / Usage | Stack |
| --- | --- | --- | --- | --- |
| `potion` | Potion | Consumable | HP `+6` | No |
| `herb` | Herb | Consumable | HP `+3` | No |
| `elixir` | Elixir | Consumable | HP `+12` | No |
| `jerky` | Jerky | Consumable | Hunger `+50` | No |
| `bread` | Bread | Consumable | Hunger `+100` | No |
| `torch` | Torch | Tool | Place to light nearby tiles. Can be broken/picked up by attacking | No |
| `flame_scroll` | Flame Scroll | Magic | MP `2`. Forward flame up to 6 tiles. Can be cast with no target | No |
| `blink_scroll` | Blink Scroll | Magic | MP `3`. Blink forward up to 4 tiles | No |
| `nova_scroll` | Nova Scroll | Magic | MP `5`. Damage all targets within radius 4 | No |
| `forge_scroll` | Forge Scroll | Magic | MP `4`. Layout:<br>`w s w`<br>`s @ s`<br>`w s w`<br>Center `@` must be on a blood stain. Strengthens equipped weapon (+1, max +6) | No |
| `wood` | Wood | Material | Crafting material | Yes |
| `stone` | Stone | Material | Crafting material | Yes |
| `iron_ingot` | Iron Ingot | Material | Crafting material | Yes |
| `hide` | Hide | Material | Crafting material | Yes |
| `string` | String | Material | Crafting material | Yes |
| `stone_axe` | Stone Axe | Tool/Weapon | Equippable, attack `+2` | No |
| `iron_sword` | Iron Sword | Weapon | Equippable, attack `+3` | No |
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
| `slime` | Slime | hostile | 6 | 3 | 0 | 100 | 1 |
| `wolf` | Wolf | hostile | 7 | 5 | 2 | 70 | 1 |
| `bat` | Bat | hostile | 6 | 4 | 1 | 60 | 1 |
| `golem` | Golem | hostile | 12 | 7 | 4 | 35 | 1 |
| `slime_brute` | Brute Slime | hostile | 14 | 8 | 4 | 55 | 3 |
| `wolf_alpha` | Alpha Wolf | hostile | 22 | 14 | 7 | 40 | 4 |
| `bat_night` | Night Bat | hostile | 20 | 13 | 6 | 35 | 4 |
| `golem_elder` | Elder Golem | hostile | 34 | 20 | 11 | 22 | 4 |
| `bandit` | Bandit | hostile | 24 | 15 | 8 | 28 | 4 |
| `traveler` | Traveler | neutral | 6 | 1 | 1 | 0 | fixed spawn |

Traveler behavior:
- Moves once every 2 turns until attacked.
- After being attacked, enters flee mode and moves away from player.
- Bump-talk before attacked uses normal line; while fleeing it uses flee line.
- Death cry log is for neutral NPCs only.
- Drops `bread` with `40%` chance on defeat.

## Drops

Hostile enemy kill drop:
- Shared global drop table is removed.
- Each enemy spawns with carried items from its own `loot` definition (`data/game_data.toml`).
- On defeat, one of that enemy's carried items is chosen at random and then checked against that entry's `drop_chance`.
- If a loot entry has `equip_as_weapon = true`, the enemy can spawn with that weapon equipped.

Examples of current hostile `loot` definitions:

| Enemy ID | Loot entries (`item: carry% / drop%`) |
| --- | --- |
| `slime` | `herb: 22 / 70`, `potion: 10 / 55` |
| `wolf` | `hide: 55 / 75`, `bread: 14 / 50` |
| `bat` | `herb: 14 / 45`, `string: 24 / 65` |
| `golem` | `stone: 70 / 80`, `iron_ingot: 26 / 60` |
| `wolf_alpha` | `hide: 64 / 80`, `bread: 22 / 50`, `elixir: 6 / 45` |
| `bat_night` | `blink_scroll: 14 / 55`, `string: 30 / 66` |
| `golem_elder` | `iron_ingot: 42 / 72`, `nova_scroll: 10 / 52`, `elixir: 12 / 62` |
| `bandit` | `iron_sword: 85 / 70 (equipable)`, `potion: 24 / 58`, `bread: 28 / 62` |
