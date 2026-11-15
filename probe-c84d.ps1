Get-CimInstance Win32_Process -Filter "Name like 'continuum%'" | ForEach-Object {
  $ct = $_.CreationTime.ToString('yyyy-MM-dd HH:mm:ss')
  Write-Output ("{0} | PID {1} | Created {2}" -f $_.Name, $_.ProcessId, $ct)
}
Write-Output '---EXE---'
Get-Item 'D:\continuum-cold\cargo-target\release\continuum-core-server.exe' | ForEach-Object { Write-Output ("Size {0} bytes | LastWriteTime {1:yyyy-MM-dd HH:mm:ss K}" -f $_.Length, $_.LastWriteTime) }
Write-Output '---NOW---'
Get-Date -Format 'yyyy-MM-dd HH:mm:ss K'
