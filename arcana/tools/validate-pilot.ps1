param([ValidateSet('performance','high','cinematic')][string]$Quality='high',[switch]$KeepRunning)
$ErrorActionPreference='Stop'
$forkPath=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$started=& (Join-Path $PSScriptRoot 'start-validation.ps1') -Quality $Quality | ConvertFrom-Json
$runPath=$started.run
$ctl=Join-Path $runPath 'bin\horizon-ctl.exe'
$endpoint=Join-Path $runPath 'control.json'
function Invoke-Control([string[]]$ControlArgs) {
    $reply=& $ctl --socket $endpoint --timeout 30 @ControlArgs
    if($LASTEXITCODE){throw ($reply -join "`n")}
    $reply | ConvertFrom-Json
}
try {
    $deadline=[DateTime]::UtcNow.AddSeconds(20)
    while(-not(Test-Path -LiteralPath $endpoint) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 250}
    Invoke-Control @('wait-ready') | Out-Null
    foreach($command in @(
        'fill -16 74 -16 16 74 30 minecraft:stone',
        'fill -16 75 -16 16 85 30 minecraft:air',
        'fill -5 75 14 5 82 14 minecraft:deepslate_bricks',
        'time set 16000',
        'gamerule minecraft:spawn_mobs false',
        'summon minecraft:iron_golem 0 75 5 {NoAI:1b,PersistenceRequired:1b}'
    )){Invoke-Control @('command',$command) | Out-Null}
    Invoke-Control @('tp','0.5','75','-10') | Out-Null
    Invoke-Control @('look','0','0') | Out-Null
    Invoke-Control @('key','f5','tap') | Out-Null
    Invoke-Control @('arcana-demo') | Out-Null
    $before=Invoke-Control @('mobs','40')
    $before | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $runPath 'mobs-before.json')
    Invoke-Control @('stats','reset') | Out-Null
    Invoke-Control @('screenshot',(Join-Path $runPath 'baseline.png')) | Out-Null
    Invoke-Control @('key','v','tap') | Out-Null
    $timer=[Diagnostics.Stopwatch]::StartNew()
    foreach($shot in @(@{name='preparation';time=1.5},@{name='release';time=3.6},@{name='impact';time=4.0},@{name='aftermath';time=5.5})) {
        $delay=[int](($shot.time-$timer.Elapsed.TotalSeconds)*1000)
        if($delay -gt 0){Start-Sleep -Milliseconds $delay}
        Invoke-Control @('screenshot',(Join-Path $runPath ($shot.name+'.png'))) | Out-Null
    }
    $after=Invoke-Control @('mobs','40')
    $stats=Invoke-Control @('stats')
    $after | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $runPath 'mobs-after.json')
    $stats | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $runPath 'frame-stats.json')
    @{run=$runPath;quality=$Quality;exe_sha256=(Get-FileHash -LiteralPath (Join-Path $runPath 'bin\horizon-client.exe') -Algorithm SHA256).Hash;
        screenshots=@('baseline','preparation','release','impact','aftermath');result='captured_requires_visual_review'} |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $runPath 'report.json')
    @{run=$runPath;quality=$Quality;result='captured_requires_visual_review'} | ConvertTo-Json -Compress
} finally {
    if(-not $KeepRunning -and (Test-Path -LiteralPath $endpoint)) {
        try{Invoke-Control @('quit') | Out-Null}catch{Write-Warning 'Test game did not acknowledge quit'}
    }
}
