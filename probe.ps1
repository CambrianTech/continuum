$now = [DateTime]::UtcNow.ToString("yyyy-MM-dd HH:mm:ss")
Write-Output ("NOW=" + $now)
$core = Get-CimInstance Win32_Process -Filter "Name='continuum-core-server.exe'"
if ($core) { foreach ($x in $core) { $ct = [ManagementDateTimeConverter]::ToDateTime($x.CreationTime); Write-Output ("CORE PID=" + $x.ProcessId + " CreatedUTC=" + $ct.ToUniversalTime().ToString("yyyy-MM-dd HH:mm:ss")) } } else { Write-Output "CORE=ABSENT" }
$cli = Get-CimInstance Win32_Process -Filter "Name='continuum.exe'"
if ($cli) { foreach ($x in $cli) { $ct = [ManagementDateTimeConverter]::ToDateTime($x.CreationTime); Write-Output ("CLIENT PID=" + $x.ProcessId + " CreatedUTC=" + $ct.ToUniversalTime().ToString("yyyy-MM-dd HH:mm:ss")) } }
$found = @()
$directs = @(
  "C:\Users\joelt\.continuum\deploy-request.json",
  "C:\Users\joelt\.airc\deploy-request.json",
  "C:\Users\joelt\.airc\worktrees\c84d885a\deploy-request.json"
)
foreach ($p in $directs) { if (Test-Path -LiteralPath $p) { $found += (Get-Item -LiteralPath $p) } }
foreach ($root in @("C:\Users\joelt\.airc", "C:\Users\joelt\.continuum")) {
  if (-not (Test-Path -LiteralPath $root)) { continue }
  foreach ($s in (Get-ChildItem -Path $root -Directory -ErrorAction SilentlyContinue)) {
    try { $d = Get-ChildItem -Path $s.FullName -File -Filter "deploy-request.json" -ErrorAction SilentlyContinue; if ($d) { $found += $d } } catch {}
  }
}
$seen = @{}
foreach ($f in $found) {
  if ($seen.ContainsKey($f.FullName)) { continue }
  $seen[$f.FullName] = $true
  Write-Output ("DEPLOYREQ " + $f.FullName + " mtimeUTC=" + ([DateTimeOffset]::new($f.LastWriteTime).ToUniversalTime().ToString("yyyy-MM-dd HH:mm:ss")))
  $txt = Get-Content -LiteralPath $f.FullName -Raw
  if ($txt) { Write-Output ("CONTENT " + $txt.Trim()) }
}
if ($found.Count -eq 0) { Write-Output "DEPLOYREQ=not-found-in-bounded-search" }
