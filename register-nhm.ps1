$manifestPath = "$env:APPDATA\MyPass\mypass_browser_manifest.json"

# Chrome
New-Item -Path "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.mypass.mypass_browser" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.mypass.mypass_browser" -Name "(Default)" -Value $manifestPath
Write-Host "Chrome: OK"

# Edge  
New-Item -Path "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.mypass.mypass_browser" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.mypass.mypass_browser" -Name "(Default)" -Value $manifestPath
Write-Host "Edge: OK"

Write-Host ""
Write-Host "Native Messaging Host registered for Chrome, Edge"
Write-Host "Manifest: $manifestPath"
