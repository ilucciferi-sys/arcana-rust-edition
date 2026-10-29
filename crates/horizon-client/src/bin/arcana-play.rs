//! Local Arcana test-profile launcher. No downloads, accounts or global settings.
#![forbid(unsafe_code)]
use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("Arcana: {error}");
        if io::stdin().is_terminal() {
            eprintln!("Нажмите Enter, чтобы закрыть окно.");
            let _ = io::stdin().read_line(&mut String::new());
        }
        process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(feature = "arcana") {
        return Err("Соберите проект с опцией --features arcana".into());
    }
    let mut school = None;
    let mut offscreen = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--school" => {
                school = Some(
                    args.next()
                        .ok_or("После --school укажите fire, water, earth или air")?,
                )
            }
            "--offscreen" => offscreen = true,
            "--help" | "-h" => {
                println!(
                    "Arcana: Rust Edition\narcana-play [--school fire|water|earth|air]\nБез параметров: выбор тестового профиля.\nV — каст, R — выбор, X — отмена, F8 — восстановить ресурсы."
                );
                return Ok(());
            }
            _ => return Err(format!("Неизвестный параметр: {arg}").into()),
        }
    }
    let school = school.unwrap_or_else(choose_school);
    if !["fire", "water", "earth", "air"].contains(&school.as_str()) {
        return Err("Неизвестная школа магии".into());
    }
    let executable = env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or("Не удалось определить папку запуска")?;
    let root = directory
        .ancestors()
        .find(|p| p.join("research-lock.json").is_file())
        .ok_or("Запустите arcana-play из собранного репозитория Horizon")?;
    let resources = env::var_os("HORIZON_ARCANA_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".local/horizon-client/26.3"));
    if !resources.is_dir() {
        return Err(
            "Ресурсы Minecraft 26.3 ещё не подготовлены. См. русскую инструкцию README.md".into(),
        );
    }
    let game = directory.join(if cfg!(windows) {
        "horizon-client.exe"
    } else {
        "horizon-client"
    });
    if !game.is_file() {
        return Err(
            "Рядом с arcana-play отсутствует horizon-client. Выполните arcana/dev.ps1 build".into(),
        );
    }
    let profile_root = env::var_os("ARCANA_PLAYER_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".local/arcana-player"));
    let profile = absolute(&profile_root)?.join(&school);
    fs::create_dir_all(&profile)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(profile.join("launcher.lock"))?;
    lock.try_lock()
        .map_err(|_| "Этот тестовый профиль уже запущен")?;
    let mut command = Command::new(game);
    command
        .current_dir(&profile)
        .args([
            "--world",
            "ArcanaLab",
            "--creative",
            "--test-terrain",
            "--render-distance",
            "6",
            "--width",
            "1280",
            "--height",
            "720",
            "--name",
            "ArcanaMage",
            "--control",
            "--control-socket",
        ])
        .arg(profile.join("control.json"))
        .arg("--resources")
        .arg(resources);
    for (key, name) in [
        ("HORIZON_HOME", "game"),
        ("HORIZON_LAUNCHER_HOME", "launcher"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("HORIZON_LAUNCHER_TRASH", "trash"),
        ("HORIZON_LAUNCHER_CLIPBOARD", "clipboard"),
        ("HORIZON_LAUNCHER_DESKTOP", "desktop"),
    ] {
        let path = profile.join(name);
        fs::create_dir_all(&path)?;
        command.env(key, path);
    }
    command
        .env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1")
        .env("HORIZON_NO_AUDIO", "1")
        .env("HORIZON_NO_DISCORD", "1")
        .env("ARCANA_DEMO_SCHOOL", &school);
    if offscreen {
        command.arg("--offscreen");
    }
    let log = fs::File::create(profile.join("session.log"))?;
    command
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .stdin(Stdio::null());
    println!(
        "Arcana: Rust Edition — тестовый профиль «{}»",
        match school.as_str() {
            "fire" => "Огонь",
            "water" => "Вода",
            "earth" => "Земля",
            _ => "Воздух",
        }
    );
    println!("V — заклинание, R — выбор, X — отмена, F8 — восстановить ману и перезарядки.");
    println!("Мир и прогресс: {}", profile.display());
    let status = command.status()?;
    drop(lock);
    if !status.success() {
        return Err(format!("Игра завершилась с ошибкой: {status}").into());
    }
    Ok(())
}
fn choose_school() -> String {
    println!(
        "Arcana: Rust Edition\n\nВыберите отдельный тестовый профиль:\n1 — Огонь\n2 — Вода\n3 — Земля\n4 — Воздух"
    );
    print!("Номер профиля [1]: ");
    let _ = io::stdout().flush();
    let mut answer = String::new();
    let _ = io::stdin().read_line(&mut answer);
    match answer.trim() {
        "2" => "water",
        "3" => "earth",
        "4" => "air",
        _ => "fire",
    }
    .into()
}
fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.into())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}
