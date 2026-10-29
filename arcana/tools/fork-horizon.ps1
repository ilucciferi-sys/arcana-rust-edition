# Use the existing Git credential, keep it in memory, and send it only to GitHub.
# No interactive sign-in and no credential in command arguments, logs or files.
$ErrorActionPreference='Stop'
$env:GCM_INTERACTIVE='Never'
$env:GIT_TERMINAL_PROMPT='0'
$credentialLines="protocol=https`nhost=github.com`n`n" | & git -c credential.interactive=never credential fill
if($LASTEXITCODE){throw 'No usable existing GitHub credential'}
$passwordLine=$credentialLines | Where-Object {$_.StartsWith('password=')} | Select-Object -First 1
if(-not $passwordLine){throw 'GitHub credential has no token'}
$headers=@{Authorization=('Bearer '+$passwordLine.Substring(9));Accept='application/vnd.github+json'}
$login=(Invoke-RestMethod -Uri 'https://api.github.com/user' -Headers $headers).login
$name='arcana-rust-edition'
try {
    $fork=Invoke-RestMethod -Uri "https://api.github.com/repos/$login/$name" -Headers $headers
} catch {
    if([int]$_.Exception.Response.StatusCode -ne 404){throw}
    $body=@{name=$name;default_branch_only=$true} | ConvertTo-Json -Compress
    $fork=Invoke-RestMethod -Method Post -Uri 'https://api.github.com/repos/horizon-rust/horizon/forks' -Headers $headers -ContentType 'application/json' -Body $body
}
if(-not $fork.fork -or $fork.parent.full_name -ne 'horizon-rust/horizon'){throw 'Destination is not the expected Horizon fork'}
@{html_url=$fork.html_url;clone_url=$fork.clone_url} | ConvertTo-Json -Compress
