# Run the native launcher in a new profile root; never touch a player's profile.
$ErrorActionPreference='Stop'
$forkPath=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$validationRoot=Join-Path $forkPath ('.local\starter-validation\'+[guid]::NewGuid().ToString('N'))
$bin=Join-Path $validationRoot 'bin'
New-Item -ItemType Directory -Force -Path $bin | Out-Null
foreach($name in @('arcana-play.exe','horizon-client.exe','horizon-ctl.exe')){Copy-Item -LiteralPath (Join-Path $forkPath ('target\fast\'+$name)) -Destination $bin}
$env:ARCANA_PLAYER_ROOT=Join-Path $validationRoot 'profiles'
$script:ctl=Join-Path $bin 'horizon-ctl.exe'
$script:endpoint=''
function Invoke-Game([string[]]$Words) {
    $raw=& $script:ctl --socket $script:endpoint --timeout 30 @Words
    if($LASTEXITCODE){throw ($raw -join "`n")}
    $raw | ConvertFrom-Json
}
function Wait-Magic([string]$School) {
    $until=[DateTime]::UtcNow.AddSeconds(12)
    do {
        $status=Invoke-Game @('arcana-status')
        if($status.snapshot.state.school -eq $School){return $status.snapshot}
        Start-Sleep -Milliseconds 100
    }while([DateTime]::UtcNow -lt $until)
    throw "Magic snapshot not ready for $School"
}
function Start-Profile([string]$School) {
    $profile=Join-Path $env:ARCANA_PLAYER_ROOT $School
    $script:endpoint=Join-Path $profile 'control.json'
    if(Test-Path -LiteralPath $script:endpoint){Remove-Item -LiteralPath $script:endpoint}
    $launcher=Start-Process -FilePath (Join-Path $bin 'arcana-play.exe') -ArgumentList @('--school',$School,'--offscreen') -WorkingDirectory $validationRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $validationRoot ($School+'-launcher.log')) -RedirectStandardError (Join-Path $validationRoot ($School+'-launcher-error.log'))
    $until=[DateTime]::UtcNow.AddSeconds(25)
    while(-not(Test-Path -LiteralPath $script:endpoint) -and [DateTime]::UtcNow -lt $until){Start-Sleep -Milliseconds 250}
    Invoke-Game @('wait-ready') | Out-Null
    Wait-Magic $School | Out-Null
    return $launcher
}
$reports=@()
foreach($school in @('fire','water','earth','air')) {
    $launcher=$null
    try {
        $launcher=Start-Profile $school
        foreach($command in @('gamerule minecraft:spawn_mobs false','fill -12 74 -12 12 74 16 minecraft:stone','fill -12 75 -12 12 84 16 minecraft:air','time set 1000')){Invoke-Game @('command',$command) | Out-Null}
        Invoke-Game @('tp','0.5','75','-3') | Out-Null
        Invoke-Game @('look','0','0') | Out-Null
        $fire=if($school -eq 'water'){',Fire:200s'}else{''}
        Invoke-Game @('command',('summon minecraft:iron_golem 0 75 3 {NoAI:1b,PersistenceRequired:1b'+$fire+'}')) | Out-Null
        if($school -eq 'fire'){Invoke-Game @('key','r','tap') | Out-Null;Start-Sleep -Milliseconds 150}
        $before=Invoke-Game @('arcana-status')
        $before | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $validationRoot ($school+'-magic-before.json'))
        Invoke-Game @('screenshot',(Join-Path $validationRoot ($school+'-hud.png'))) | Out-Null
        if($school -eq 'air') {
            Invoke-Game @('command','summon minecraft:arrow 1.0 77.0 -1.0 {Motion:[0.0d,0.4d,0.15d]}') | Out-Null
            Invoke-Game @('simulation') | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $validationRoot 'air-projectiles-before.json')
        }
        Invoke-Game @('key','v','tap') | Out-Null
        $delay=switch($school){'fire'{410}'water'{570}'earth'{650}'air'{350}}
        Start-Sleep -Milliseconds $delay
        Invoke-Game @('screenshot',(Join-Path $validationRoot ($school+'-spell.png'))) | Out-Null
        Start-Sleep -Milliseconds 250
        $mobs=Invoke-Game @('mobs','30')
        $golem=$mobs.near | Where-Object {$_.kind -eq 'minecraft:iron_golem'} | Select-Object -First 1
        if(-not $golem -or $golem.health -ge 100){throw "No $school damage reached the golem"}
        if($school -eq 'water' -and $golem.burning){throw 'Water did not extinguish the golem'}
        $effects=Invoke-Game @('command','data get entity @e[type=minecraft:iron_golem,limit=1] active_effects')
        $effects | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $validationRoot ($school+'-effects.json'))
        if($school -eq 'water' -and ($effects.chat -join ' ') -notmatch 'slowness'){throw 'Water did not apply slowness'}
        $after=Invoke-Game @('arcana-status')
        $after | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $validationRoot ($school+'-magic-after.json'))
        if($after.snapshot.state.xp -le $before.snapshot.state.xp){throw "No XP awarded for $school"}
        if($school -eq 'air') {
            Invoke-Game @('simulation') | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $validationRoot 'air-projectiles-after.json')
            $session=Get-Content -LiteralPath (Join-Path $env:ARCANA_PLAYER_ROOT 'air\session.log') -Raw
            if($session -notmatch 'Arcana gust: damaged=1, discarded=1'){throw 'Gust did not discard the moving arrow'}
        }
        $reports+=@{school=$school;health=$golem.health;burning=$golem.burning;xp=$after.snapshot.state.xp}
        if($school -eq 'fire') {
            Invoke-Game @('key','f8','tap') | Out-Null;Invoke-Game @('key','r','tap') | Out-Null;Start-Sleep -Milliseconds 150
            Invoke-Game @('key','v','tap') | Out-Null;Start-Sleep -Milliseconds 400;Invoke-Game @('key','x','tap') | Out-Null
            Start-Sleep -Milliseconds 150
        }
    } finally {
        if(Test-Path -LiteralPath $script:endpoint){try{Invoke-Game @('quit') | Out-Null}catch{Write-Warning $_}}
        if($launcher){$launcher.WaitForExit(15000) | Out-Null}
    }
}
try {
    $launcher=Start-Profile 'fire'
    $reloaded=Wait-Magic 'fire'
    if($reloaded.state.xp -le 1600 -or $reloaded.state.selected -ne 'solar_spear' -or $reloaded.state.cooldowns.solar_spear -le 0){throw 'Progress or reserved cooldown did not survive restart'}
    $reloaded | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $validationRoot 'fire-reloaded.json')
    @{root=$validationRoot;profiles=$reports;restart='passed'} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $validationRoot 'report.json')
    Get-Content -LiteralPath (Join-Path $validationRoot 'report.json')
} finally {
    if(Test-Path -LiteralPath $script:endpoint){try{Invoke-Game @('quit') | Out-Null}catch{Write-Warning $_}}
    if($launcher){$launcher.WaitForExit(15000) | Out-Null}
}
