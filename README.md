# Abyss

Terminal roguelike written in Rust．

## 開発環境

開発には `nix develop` を使用します。ツールと依存ライブラリは `flake.nix`、
バージョンは `flake.lock` で管理します。以下の Cargo コマンドは開発シェル内で実行してください。

```sh
nix develop
# 開発シェル内
cargo check
```

Git 管理の場合は `git add flake.nix flake.lock` で追跡対象にすると、
`nix develop` / `nix develop -c cargo test` をそのまま使えます。
Nix の `nix-command` / `flakes` を有効にしてください。

## Build

```bash
cargo run
```

Debug mode:

```bash
cargo run -- --debug
```

## Controls

- Move: `W/A/S/D`
- Diagonal move: `Q/E/Z/C`
- Attack / Confirm: `F`
- Wait: `Space`
- Menu / Back: `V`
- Quickbar: `1..9,0`

## Features

- Procedural floor generation with mixed natural and artificial zones
- Multi-language UI: Japanese / English / Latin
- Ancient weapons and shrine interactions
- Debug console and placement / teleport tools

## Notes

- Public repository build and implementation reference are included．
- Private lore and design notes are intentionally excluded from version control．

## References

- Japanese reference: [docs/game_reference_ja.md](docs/game_reference_ja.md)
- English reference: [docs/game_reference_en.md](docs/game_reference_en.md)
