# Invoked only on GitHub-hosted runners. Copies audited assets, no legacy runtime.
param([Parameter(Mandatory)][string]$Destination)
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -ne 'true'){throw 'Wallpaper asset packaging is hosted only'}
$root=Join-Path $Destination 'wallpaper'
foreach($name in 'shaders','assets','thumbs'){New-Item -ItemType Directory -Path (Join-Path $root $name) -Force|Out-Null}
Copy-Item assets/shaders/*.frag (Join-Path $root 'shaders')
foreach($name in 'aquarium','commute','summer-porch','boxing-ring'){Copy-Item "prototype/$name.frag" (Join-Path $root 'shaders')}
Copy-Item native/wallpaper/cats-native.frag,native/wallpaper/post.frag,native/wallpaper/meadow-kite-native.frag (Join-Path $root 'shaders')
Copy-Item prototype/assets/*.png (Join-Path $root 'assets')
$manifest=@(Get-ChildItem -LiteralPath $root -File -Recurse|Where-Object Name -NE 'ASSET-SHA256.json'|ForEach-Object {[ordered]@{path=[IO.Path]::GetRelativePath($root,$_.FullName);sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}})
$manifest|ConvertTo-Json -Depth 4|Set-Content (Join-Path $root 'ASSET-SHA256.json') -Encoding UTF8
