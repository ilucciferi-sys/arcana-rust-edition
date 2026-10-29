# Real server-thread input and world checks, always in new isolated profiles.
$ErrorActionPreference='Stop'
$forkPath=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$validationRoot=Join-Path $forkPath ('.local\ritual-validation\'+[guid]::NewGuid().ToString('N'))
$bin=Join-Path $validationRoot 'bin'
New-Item -ItemType Directory -Force -Path $bin | Out-Null
foreach($name in @('arcana-play.exe','horizon-client.exe','horizon-ctl.exe')){Copy-Item -LiteralPath (Join-Path $forkPath ('target\fast\'+$name)) -Destination $bin}
$script:endpoint=''
function Invoke-Game([string[]]$Words) {
    $raw=& (Join-Path $bin 'horizon-ctl.exe') --socket $script:endpoint --timeout 30 @Words
    if($LASTEXITCODE){throw ($raw -join "`n")}
    $raw | ConvertFrom-Json
}
function Wait-State([scriptblock]$Condition,[int]$Seconds=12) {
    $until=[DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        $value=Invoke-Game @('arcana-status')
        if(& $Condition $value){return $value}
        Start-Sleep -Milliseconds 100
    }while([DateTime]::UtcNow -lt $until)
    $value | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $validationRoot 'failed-state.json')
    Invoke-Game @('state') | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $validationRoot 'failed-player.json')
    throw 'Timed out waiting for expected ritual state'
}
function Start-Profile([string]$School) {
    $env:ARCANA_PLAYER_ROOT=Join-Path $validationRoot ('profiles\'+$School)
    $profile=Join-Path $env:ARCANA_PLAYER_ROOT 'awakening'
    $script:endpoint=Join-Path $profile 'control.json'
    if(Test-Path -LiteralPath $script:endpoint){Remove-Item -LiteralPath $script:endpoint}
    $launcher=Start-Process -FilePath (Join-Path $bin 'arcana-play.exe') -ArgumentList @('--awakening','--offscreen') -WorkingDirectory $validationRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $validationRoot ($School+'-launcher.log')) -RedirectStandardError (Join-Path $validationRoot ($School+'-launcher-error.log'))
    $until=[DateTime]::UtcNow.AddSeconds(25)
    while(-not(Test-Path -LiteralPath $script:endpoint) -and [DateTime]::UtcNow -lt $until){Start-Sleep -Milliseconds 200}
    Invoke-Game @('wait-ready') | Out-Null
    Wait-State {param($s) $null -ne $s.snapshot} | Out-Null
    return $launcher
}
function Command([string]$Text){Invoke-Game @('command',$Text) | Out-Null}
function Block([int]$X,[int]$Y,[int]$Z,[string]$Name){Command "setblock $X $Y $Z minecraft:$Name"}
function Build-Ritual([string]$School,[int]$Y) {
    Command 'gamerule minecraft:spawn_mobs false'
    Command "fill -5 $Y -5 5 $Y 5 minecraft:stone"
    $above=$Y+1;$top=$Y+12
    Command "fill -5 $above -5 5 $top 5 minecraft:air"
    Command 'time set 1000'
    $names=switch($School){'fire'{@('obsidian','obsidian','magma_block','obsidian')}'earth'{@('polished_deepslate','deepslate','copper_block','iron_ore')}'air'{@('quartz_block','smooth_quartz','quartz_block','glass')}'water'{@('prismarine','prismarine_bricks','sea_lantern')}}
    Block 0 $Y 0 $names[0]
    if($School -eq 'water') {
        foreach($x in -1..1){foreach($z in -1..1){if($x -ne 0 -or $z -ne 0){Block $x $Y $z $names[1];Block $x $above $z 'water[level=0]'}}}
        foreach($x in @(-2,2)){foreach($z in @(-2,2)){Block $x $Y $z $names[2]}}
    }else{
        foreach($distance in @(1,2)){
            foreach($pair in @(@($distance,0),@(-$distance,0),@(0,$distance),@(0,-$distance))){
                Block $pair[0] $Y $pair[1] $names[$distance]
                if($School -eq 'air' -and $distance -eq 2){Block $pair[0] $above $pair[1] 'end_rod'}
            }
        }
        $d=if($School -eq 'air'){1}else{2}
        foreach($x in @(-$d,$d)){foreach($z in @(-$d,$d)){
            Block $x $Y $z $names[3]
            if($School -eq 'fire'){Block $x $above $z 'campfire[lit=true]'}
        }}
        if($School -eq 'fire'){Block 3 $Y 0 'lava[level=0]'}
    }
    Command "tp @s 0.5 $above 0.5"
    Invoke-Game @('look','0','0') | Out-Null
    Start-Sleep -Milliseconds 600
    Command 'kill @e[type=minecraft:item]'
    Command 'clear @s'
    Invoke-Game @('state') | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $validationRoot ($School+'-player-before.json'))
}
function Sneak([string]$Action){Invoke-Game @('key','shift',$Action) | Out-Null}
function Assert-Unbound {
    $s=Invoke-Game @('arcana-status')
    if($s.snapshot.state.school -or $s.snapshot.ritual){throw 'Invalid ritual granted magic or remained active'}
}
$reports=@()
foreach($school in @('fire','water','earth','air')){
    $launcher=$null
    try{
        $launcher=Start-Profile $school
        Assert-Unbound
        $y=switch($school){'earth'{64}'air'{120}default{74}}
        Build-Ritual $school $y
        if($school -eq 'fire'){
            Command 'give @s minecraft:stick'
            Sneak 'down';Start-Sleep -Milliseconds 1400;Assert-Unbound;Sneak 'up'
            Command 'clear @s'
            Block 1 $y 0 'air'
            Sneak 'down';Start-Sleep -Milliseconds 1400;Assert-Unbound;Sneak 'up'
            Block 1 $y 0 'obsidian'
            Sneak 'down';Wait-State {param($s) $null -ne $s.snapshot.ritual} | Out-Null
            Sneak 'up';Block 1 $y 0 'air'
            Wait-State {param($s) $null -eq $s.snapshot.ritual} | Out-Null
            Assert-Unbound
            Block 1 $y 0 'obsidian'
            Sneak 'down';Wait-State {param($s) $null -ne $s.snapshot.ritual} | Out-Null;Sneak 'up'
            Command 'tp @s 5.5 75 0.5'
            Wait-State {param($s) $null -eq $s.snapshot.ritual} | Out-Null
            Assert-Unbound
            Command 'tp @s 0.5 75 0.5'
        }
        Invoke-Game @('key','f5','tap') | Out-Null
        Sneak 'down'
        $started=Wait-State {param($s) $null -ne $s.snapshot.ritual}
        Sneak 'up'
        $during=Wait-State {param($s) $s.snapshot.ritual.progress -ge 70}
        Invoke-Game @('screenshot',(Join-Path $validationRoot ($school+'-ritual.png'))) | Out-Null
        if($school -eq 'air'){
            Wait-State {param($s) $s.snapshot.ritual.progress -ge 95} | Out-Null
            Invoke-Game @('effects') | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $validationRoot 'air-levitation.json')
        }
        $after=Wait-State {param($s) $s.snapshot.state.school -eq $school}
        if($after.snapshot.state.xp -ne 0 -or $after.snapshot.state.mana -ne 100){throw 'Awakening did not grant original level-one resources'}
        Invoke-Game @('screenshot',(Join-Path $validationRoot ($school+'-awakened.png'))) | Out-Null
        Invoke-Game @('key','v','tap') | Out-Null
        $cast=Wait-State {param($s) $s.snapshot.state.mana -lt 100}
        @{started=$started;during=$during;awakened=$after;cast=$cast} | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $validationRoot ($school+'-states.json'))
        $reports+=@{school=$school;xp=$after.snapshot.state.xp;selected=$after.snapshot.state.selected;cast_mana=$cast.snapshot.state.mana}
    }finally{
        if(Test-Path -LiteralPath $script:endpoint){try{Sneak 'up';Invoke-Game @('quit') | Out-Null}catch{Write-Warning $_}}
        if($launcher){$launcher.WaitForExit(15000) | Out-Null}
    }
}
try{
    $launcher=Start-Profile 'fire'
    $saved=Wait-State {param($s) $s.snapshot.state.school -eq 'fire'}
    if($saved.snapshot.state.xp -ne 0 -or $saved.snapshot.state.selected -ne 'spark' -or $saved.snapshot.ritual){throw 'Ritual progress did not persist correctly'}
    $saved | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $validationRoot 'reloaded.json')
    @{root=$validationRoot;profiles=$reports;restart='passed';invalid_inputs='passed'} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $validationRoot 'report.json')
    Get-Content -LiteralPath (Join-Path $validationRoot 'report.json')
}finally{
    if(Test-Path -LiteralPath $script:endpoint){try{Invoke-Game @('quit') | Out-Null}catch{Write-Warning $_}}
    if($launcher){$launcher.WaitForExit(15000) | Out-Null}
}
