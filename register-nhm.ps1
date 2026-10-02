$manifestPath = "$env:APPDATA\MyPass\mypass_browser_manifest.json"

# Chrome
New-Item -Path "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.mypass.mypass_browser" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.mypass.mypass_browser" -Name "(Default)" -Value $manifestPath
Write-Host "Chrome: OK"

# Edge  
New-Item -Path "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.mypass.mypass_browser" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.mypass.mypass_browser" -Name "(Default)" -Value $manifestPath
Write-Host "Edge: OK"

# Firefox (uses JSON file, not registry)
$firefoxDir = "$env:APPDATA\Mozilla\Firefox"
if (-not (Test-Path $firefoxDir)) { New-Item -ItemType Directory -Force -Path $firefoxDir | Out-Null }
$firefoxManifestDir = "$env:APPDATA\Mozilla\NativeMessagingHosts"
New-Item -ItemType Directory -Force -Path $firefoxManifestDir | Out-Null
Copy-Item -Path $manifestPath -Destination "$firefoxManifestDir\com.mypass.mypass_browser.json" -Force
Write-Host "Firefox: OK"

Write-Host ""
Write-Host "Native Messaging Host registered for Chrome, Edge, Firefox"
Write-Host "Manifest: $manifestPath"
