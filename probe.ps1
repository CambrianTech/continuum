$procs = Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'continuum' }
if (-not $procs) { Write-Output "NO continuum process found" }
foreach ($p in $procs) {
  try {
    $created = [Management.ManagementDateTimeConverter]::ToDateTimeMilliseconds($p.CreationTime).ToUniversalTime().ToString('yyyy-MM-dd HH:mm:ss')
  } catch {
    $created = 'parse-error:' + $_.Exception.Message
  }
  Write-Output ("PID=" + $p.ProcessId + " CreatedUTC=" + $created + " Name=" + $p.Name)
}
