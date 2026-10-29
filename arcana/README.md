# Arcana: Rust Edition — migration workspace

Это начало переноса Arcana 1.8.1, а не готовая игра. Отдельный локальный форк
Horizon находится в родительском каталоге; upstream закреплён на
`7ebf7d26cbe58d65ef873767c639f144dc403554`, ветка `codex/arcana-rust`.
Создан [отдельный GitHub-форк](https://github.com/ilucciferi-sys/arcana-rust-edition).
Код прототипа публикуется в ветке `codex/arcana-rust`. Исходный Java-проект находится за пределами
форка и не редактируется.

- `crates/arcana-core`: каталог 76 заклинаний, базовая прогрессия и мана,
  JSON-сохранения нового формата, серверные фазы каста, отмена и сетевые события,
  расчёт Солнечного копья через интерфейс авторитетного мира.
- `crates/horizon-vfx`: независимое от Arcana расширение рендера wgpu 30:
  GPU-пул частиц, кольца подготовки, луч, ударная волна, HDR emission и bloom.
- `../crates/horizon-client/src/game/arcana.rs`: экспериментальный адаптер
  к серверу и клиенту Horizon. Включается только Cargo feature `arcana`.
- `assets`: побайтовые копии авторских ресурсов Arcana, без файлов Minecraft.
- `tools/audit.py`: инвентаризация исходного проекта и проверка копий ресурсов.

Из каталога пока исполняется только `solar_spear`. Другие механики возвращают
`UnsupportedMechanic` до расхода маны. Наличие определения не означает готовый перенос.

```powershell
# После настройки Rust и компилятора для Windows:
cargo test --manifest-path arcana/Cargo.toml
cargo build --profile fast -p horizon-client --no-default-features --features arcana --bins
```

Подготовка ресурсов Horizon требует установленной Minecraft 26.3 и Java 25:

```powershell
cargo run -p horizon-prepare -- --prepare --minecraft-dir <minecraft-dir> --server-jar <server.jar>
```

Все игровые прогоны должны использовать отдельные временные каталоги данных:
см. [AGENTS.md](../AGENTS.md). Не запускайте проверку в личных сохранениях.

В экспериментальной сборке контроллер `arcana-demo` отправляет запрос выдачи
уровня 5, маны и сброса КД; сервер разрешает его только оператору. Это инструмент
проверки, а не замена ритуалов. `V` запускает копьё, `X` отменяет подготовку.
`ARCANA_QUALITY=performance|high|cinematic` выбирает 4096/16384/49152 частиц;
это бюджеты прототипа, ещё не результаты профилирования. `ARCANA_REDUCED_EFFECTS=1`
снижает яркость. Camera shake/FOV имеют настройки в библиотеке, но пока не
подключены к игровой камере.

Ограничения и следующие этапы: [дорожная карта](docs/ROADMAP.md).
