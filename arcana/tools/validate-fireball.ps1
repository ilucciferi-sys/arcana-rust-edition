# Native server-thread projectile, explosion and lifecycle checks with two clients.
$ErrorActionPreference='Stop'
$hostRun=$null;$observerRun=$null
function Invoke-Client([string]$Root,[string[]]$Words) {
    $raw=& (Join-Path $Root 'bin\horizon-ctl.exe') --socket (Join-Path $Root 'control.json') --timeout 30 @Words
    if($LASTEXITCODE){throw ($raw -join "`n")}
    $raw | ConvertFrom-Json
}
function Ready([string]$Root) {
    $until=[DateTime]::UtcNow.AddSeconds(25)
    while(-not(Test-Path -LiteralPath (Join-Path $Root 'control.json')) -and [DateTime]::UtcNow -lt $until){Start-Sleep -Milliseconds 200}
    Invoke-Client $Root @('wait-ready') | Out-Null
}
function Wait-State([string]$Root,[scriptblock]$Condition) {
    $until=[DateTime]::UtcNow.AddSeconds(10)
    do{
        $s=Invoke-Client $Root @('arcana-status')
        if(& $Condition $s){return $s}
        Start-Sleep -Milliseconds 75
    }while([DateTime]::UtcNow -lt $until)
    $s | ConvertTo-Json -Depth 18 | Set-Content -LiteralPath (Join-Path $Root 'fireball-failed-state.json')
    throw 'Expected fireball state was not reached'
}
function Command([string]$Text){Invoke-Client $hostRun @('command',$Text) | Out-Null}
function Magic { (Invoke-Client $hostRun @('arcana-status')).snapshot }
function Select-Fireball {
    for($i=0;$i -lt 12;$i++){
        if((Magic).state.selected -eq 'fireball'){return}
        Invoke-Client $hostRun @('key','r','tap') | Out-Null;Start-Sleep -Milliseconds 100
    }
    throw 'Could not select fireball'
}
function Reset-Arena {
    Invoke-Client $hostRun @('arcana-demo') | Out-Null
    Command 'kill @e[type=minecraft:iron_golem]'
    Invoke-Client $hostRun @('wait','21') | Out-Null
    Command 'fill -12 74 -12 12 74 24 minecraft:stone'
    Command 'fill -12 75 -12 12 100 24 minecraft:air'
    Command 'tp @s 0.5 75 -6.5'
    Invoke-Client $hostRun @('look','0','0') | Out-Null
    Select-Fireball
}
function Impacts {
    @(Select-String -LiteralPath @((Join-Path $hostRun 'stderr.log'),(Join-Path $hostRun 'stdout.log')) -SimpleMatch 'Arcana fireball impact:').Count
}
function No-Visual([object]$Status) {
    @($Status.spell_visuals | Where-Object {$_.spell_id -eq 'fireball'}).Count -eq 0
}
try {
    $hostRun=(& (Join-Path $PSScriptRoot 'start-validation.ps1') | ConvertFrom-Json).run;Ready $hostRun
    Command 'gamerule minecraft:spawn_mobs false';Command 'time set 1000'
    Reset-Arena
    foreach($target in @(@('primary',0.5,8.5),@('splash',2.2,8.5),@('hidden',-1.5,8.5),@('outside',4.1,8.5),@('far',0.5,12.5))){
        Command ('summon minecraft:iron_golem '+$target[1]+' 75 '+$target[2]+' {NoAI:1b,PersistenceRequired:1b,Tags:["'+$target[0]+'"]}')
    }
    Command 'data merge entity @e[tag=primary,limit=1] {NoAI:0b}'
    Command 'attribute @e[tag=primary,limit=1] minecraft:movement_speed base set 0'
    Command 'fill -1 75 8 -1 78 10 minecraft:stone'
    $initial=Invoke-Client $hostRun @('mobs','40');$ids=@{}
    $initial | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $hostRun 'fireball-mobs-before.json')
    foreach($target in @(@('primary',0.5,8.5),@('splash',2.2,8.5),@('hidden',-1.5,8.5),@('outside',4.1,8.5),@('far',0.5,12.5))){
        $m=$initial.near | Where-Object {$_.health -gt 0 -and $_.kind -eq 'minecraft:iron_golem' -and [Math]::Abs($_.pos[0]-$target[1]) -lt 0.2 -and [Math]::Abs($_.pos[2]-$target[2]) -lt 0.2} | Select-Object -First 1
        if(-not $m){throw ('Missing '+$target[0]+' target')};$ids[$target[0]]=$m.id
    }
    $port=(Invoke-Client $hostRun @('lan','open','0')).port
    $observerRun=(& (Join-Path $PSScriptRoot 'start-validation.ps1') -ServerAddress ('127.0.0.1:'+$port) | ConvertFrom-Json).run;Ready $observerRun
    Command 'tp ArcanaWitness 4.5 75 -3.5'
    Invoke-Client $observerRun @('look','22','5') | Out-Null
    $before=Magic
    Invoke-Client $hostRun @('key','v','tap') | Out-Null
    $flight=Wait-State $observerRun {param($s) @($s.spell_visuals | Where-Object {$_.spell_id -eq 'fireball' -and $_.stage -eq 'Flight' -and $_.age -ge 6}).Count -gt 0}
    Invoke-Client $observerRun @('screenshot',(Join-Path $observerRun 'fireball-flight.png')) | Out-Null
    $impact=Wait-State $observerRun {param($s) @($s.spell_visuals | Where-Object {$_.spell_id -eq 'fireball' -and $_.stage -eq 'FireballImpact'}).Count -gt 0}
    Invoke-Client $observerRun @('screenshot',(Join-Path $observerRun 'fireball-impact.png')) | Out-Null
    $after=Wait-State $hostRun {param($s) $s.snapshot.state.xp -eq $before.state.xp+4}
    $mobs=Invoke-Client $hostRun @('mobs','40');$targets=@{}
    foreach($name in $ids.Keys){$targets[$name]=$mobs.near | Where-Object {$_.id -eq $ids[$name]} | Select-Object -First 1}
    foreach($name in @('primary','splash')){
        if($targets[$name].health -gt 94 -or $targets[$name].health -le 90 -or -not $targets[$name].burning){throw "Unexpected damage or burn for $name"}
    }
    foreach($name in @('hidden','outside','far')){if($targets[$name].health -ne 100 -or $targets[$name].burning){throw "Explosion affected $name"}}
    if($targets.primary.pos[2] -le 8.6){throw 'Explosion did not move the primary target'}
    if((Impacts) -ne 1 -or $after.server_projectiles -ne 0){throw 'Expected exactly one consumed projectile and impact'}
    $baseXp=$after.snapshot.state.xp

    Reset-Arena
    Invoke-Client $hostRun @('key','v','tap') | Out-Null
    Wait-State $observerRun {param($s) @($s.spell_visuals | Where-Object {$_.spell_id -eq 'fireball' -and $_.stage -eq 'Flight' -and $_.age -ge 6}).Count -gt 0} | Out-Null
    Invoke-Client $observerRun @('screenshot',(Join-Path $observerRun 'fireball-flight-live.png')) | Out-Null
    $miss=Wait-State $hostRun {param($s) $s.server_projectiles -eq 0 -and -not $s.snapshot.cast}
    Wait-State $observerRun {param($s) No-Visual $s} | Out-Null
    if((Impacts) -ne 1 -or $miss.snapshot.state.xp -ne $baseXp){throw 'Expired fireball exploded or awarded XP'}

    Reset-Arena
    Command 'fill -2 75 2 3 81 2 minecraft:stone'
    Command 'summon minecraft:iron_golem 0.5 75 3.5 {NoAI:1b,PersistenceRequired:1b}'
    Invoke-Client $hostRun @('key','v','tap') | Out-Null
    $wall=Wait-State $observerRun {param($s) @($s.spell_visuals | Where-Object {$_.spell_id -eq 'fireball' -and $_.stage -eq 'FireballImpact'}).Count -gt 0}
    Invoke-Client $observerRun @('screenshot',(Join-Path $observerRun 'fireball-wall.png')) | Out-Null
    Start-Sleep -Milliseconds 500
    $wallMagic=Magic;$wallMobs=Invoke-Client $hostRun @('mobs','40')
    $golem=$wallMobs.near | Where-Object {$_.kind -eq 'minecraft:iron_golem' -and $_.health -gt 0} | Select-Object -First 1
    if($golem.health -ne 100 -or $golem.burning -or $wallMagic.state.xp -ne $baseXp -or (Impacts) -ne 2){throw 'Wall failed to shield the nearby golem'}

    Invoke-Client $hostRun @('arcana-demo') | Out-Null
    Start-Sleep -Milliseconds 400
    Invoke-Client $hostRun @('key','v','tap') | Out-Null;Start-Sleep -Milliseconds 150
    Invoke-Client $hostRun @('key','x','tap') | Out-Null
    $cancelled=Wait-State $hostRun {param($s) -not $s.snapshot.cast -and $s.snapshot.state.mana -eq 160 -and $s.snapshot.state.cooldowns.fireball -gt 0}
    Wait-State $observerRun {param($s) No-Visual $s} | Out-Null
    if((Impacts) -ne 2 -or $cancelled.server_projectiles -ne 0){throw 'Cancelled preparation released a projectile'}

    Reset-Arena
    Invoke-Client $hostRun @('look','0','-90') | Out-Null
    Invoke-Client $hostRun @('key','v','tap') | Out-Null
    Wait-State $observerRun {param($s) @($s.spell_visuals | Where-Object {$_.spell_id -eq 'fireball' -and $_.stage -eq 'Flight' -and $_.age -ge 6}).Count -gt 0} | Out-Null
    Command 'kill @s'
    $dead=Wait-State $hostRun {param($s) $s.server_projectiles -eq 0}
    Wait-State $observerRun {param($s) No-Visual $s} | Out-Null
    Invoke-Client $hostRun @('wait','25') | Out-Null
    Invoke-Client $hostRun @('respawn') | Out-Null
    Invoke-Client $hostRun @('wait-ready') | Out-Null
    $respawn=Wait-State $hostRun {param($s) $s.snapshot.state.school -eq 'fire'}
    if($respawn.server_projectiles -ne 0 -or (Impacts) -ne 2){throw 'A stale fireball resumed after respawn'}
    $observer=Invoke-Client $observerRun @('arcana-status')
    if($observer.snapshot.state.school -or $observer.snapshot.state.xp -ne 0){throw 'Owner progress leaked to observer'}
    @{host_run=$hostRun;observer_run=$observerRun;flight=$flight;impact=$impact;after=$after;targets=$targets;miss=$miss;wall=$wall;wall_state=$wallMagic;cancelled=$cancelled;death=$dead;respawn=$respawn;observer=$observer;visual_review='required'} | ConvertTo-Json -Depth 24 | Set-Content -LiteralPath (Join-Path $hostRun 'fireball-report.json')
    @{host_run=$hostRun;observer_run=$observerRun;combat='passed';wall='passed';miss='passed';cancellation='passed';death_respawn='passed';owner_only='passed'} | ConvertTo-Json -Compress
}finally{
    if($hostRun){try{Invoke-Client $hostRun @('server-hold','off') | Out-Null}catch{Write-Warning $_}}
    foreach($run in @($observerRun,$hostRun)){if($run -and (Test-Path -LiteralPath (Join-Path $run 'control.json'))){try{Invoke-Client $run @('quit') | Out-Null}catch{Write-Warning $_}}}
}
