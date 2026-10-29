param(
    [ValidateSet('test','build','prepare','audit','run','play')]
    [string]$Task = 'test',
    [string]$ServerJar,
    [string]$MinecraftDir = (Join-Path $env:APPDATA '.minecraft')
)
$ErrorActionPreference = 'Stop'
$forkPath = Split-Path $PSScriptRoot -Parent
$sourcePath = Split-Path $forkPath -Parent
$localRust = Join-Path $sourcePath '.tools\rust'
if (Test-Path -LiteralPath (Join-Path $localRust 'cargo\bin\cargo.exe')) {
    $env:RUSTUP_HOME = Join-Path $localRust 'rustup'
    $env:CARGO_HOME = Join-Path $localRust 'cargo'
    $env:PATH = (Join-Path $localRust 'cargo\bin') + ';' + $env:PATH
    $mingw = Get-ChildItem -LiteralPath (Join-Path $localRust 'llvm-mingw') -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($mingw) { $env:PATH = (Join-Path $mingw.FullName 'bin') + ';' + $env:PATH }
    $env:RUSTFLAGS = '-C linker=rust-lld -C linker-flavor=ld.lld'
}
Push-Location $forkPath
try {
    switch ($Task) {
        'test' { & cargo test --manifest-path arcana/Cargo.toml; if ($LASTEXITCODE) { throw 'Tests failed' } }
        'build' { & cargo build --profile fast -p horizon-client --no-default-features --features arcana --bins; if ($LASTEXITCODE) { throw 'Build failed' } }
        'play' { & .\target\fast\arcana-play.exe; if ($LASTEXITCODE) { throw 'Game exited with an error' } }
        'audit' {
            $pythonExe = Join-Path $sourcePath '.tools\python312\python.exe'
            if (-not (Test-Path -LiteralPath $pythonExe)) { $pythonExe = 'python' }
            & $pythonExe arcana/tools/audit.py --check
            if ($LASTEXITCODE) { throw 'Original-source or asset verification failed' }
        }
        'prepare' {
            if (-not $ServerJar) { $ServerJar = Join-Path $forkPath '.local\downloads\server-26.3.jar' }
            $java25 = Join-Path $MinecraftDir 'runtime\java-runtime-epsilon\windows\java-runtime-epsilon'
            if (Test-Path -LiteralPath (Join-Path $java25 'bin\javac.exe')) { $env:JAVA_HOME = $java25 }
            & cargo run -p horizon-prepare -- --prepare --minecraft-dir $MinecraftDir --server-jar $ServerJar
            if ($LASTEXITCODE) { throw 'Resource preparation failed' }
        }
        'run' {
            $runPath = Join-Path $forkPath ('.local\arcana-runs\' + [guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Path $runPath -Force | Out-Null
            foreach ($entry in @{
                HORIZON_HOME='game'; HORIZON_LAUNCHER_HOME='launcher'; XDG_DATA_HOME='data'; XDG_CONFIG_HOME='config';
                HORIZON_LAUNCHER_TRASH='trash'; HORIZON_LAUNCHER_CLIPBOARD='clipboard'; HORIZON_LAUNCHER_DESKTOP='desktop'
            }.GetEnumerator()) {
                $path = Join-Path $runPath $entry.Value
                New-Item -ItemType Directory -Path $path -Force | Out-Null
                Set-Item -LiteralPath ('Env:' + $entry.Key) -Value $path
            }
            $env:HORIZON_LAUNCHER_NO_SELF_UPDATE='1'
            $env:HORIZON_NO_AUDIO='1'
            $env:HORIZON_NO_DISCORD='1'
            Set-Content -LiteralPath (Join-Path $forkPath '.local\arcana-latest-run.txt') -Value $runPath
            $resources = Join-Path $forkPath '.local\horizon-client\26.3'
            Write-Output "Isolated run: $runPath"
            Push-Location $runPath
            try {
                & (Join-Path $forkPath 'target\fast\horizon-client.exe') --resources $resources --world ArcanaLab --creative --test-terrain --render-distance 4 --control --control-socket (Join-Path $runPath 'control.json')
                if ($LASTEXITCODE) { throw 'Game exited with an error' }
            } finally { Pop-Location }
        }
    }
} finally { Pop-Location }
