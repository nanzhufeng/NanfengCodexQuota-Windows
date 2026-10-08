[CmdletBinding()]
param([switch]$AllowUnsigned, [string]$Compiler, [string]$OutDir='dist')
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $root
$version=(Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
if(-not $Compiler){$Compiler=Join-Path $env:LOCALAPPDATA 'Programs/Inno Setup 7/ISCC.exe'}
if(-not(Test-Path -LiteralPath $Compiler)){throw '需要 Inno Setup 7 的 ISCC.exe；可使用 -Compiler 指定。'}
$names=@('NANFENG_CODEX_QUOTA_PFX_PATH','NANFENG_CODEX_QUOTA_PFX_PASSWORD','NANFENG_CODEX_QUOTA_SIGN_EXPECTED_SUBJECT','NANFENG_CODEX_QUOTA_TIMESTAMP_URL')
$values=@($names | ForEach-Object {[Environment]::GetEnvironmentVariable($_)})
$present=@($values | Where-Object {$_}).Count
$signing=$null
if($present -gt 0){
  if($present -ne $names.Count){throw '发行签名环境变量仅配置了一部分；停止构建，不回退。'}
  $signing=@{pfxPath=$values[0];pfxPassword=$values[1];expectedSubject=$values[2];timestampUrl=$values[3]}
} else {
  $fallback=Join-Path ([Environment]::GetFolderPath('UserProfile')) '.config/nanfeng-signing/NanfengCodexQuota-Windows.json'
  if(Test-Path -LiteralPath $fallback){$signing=Get-Content -LiteralPath $fallback -Raw | ConvertFrom-Json -AsHashtable}
}
if($signing){
  foreach($field in @('pfxPath','pfxPassword','expectedSubject','timestampUrl')){if(-not $signing[$field]){throw '用户级发行签名配置不完整。'}}
  $certificate=[System.Security.Cryptography.X509Certificates.X509Certificate2]::new($signing.pfxPath,$signing.pfxPassword,[System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet)
  if(-not $certificate.HasPrivateKey -or $certificate.Subject -ne $signing.expectedSubject -or $certificate.Subject -eq $certificate.Issuer){throw '签名身份不符合指定发行者，或为自签名证书；停止。'}
} elseif(-not $AllowUnsigned){throw '未找到发行签名配置。仅在明确接受未签名交付时传入 -AllowUnsigned。'}
function Invoke-Checked([string]$program,[string[]]$arguments){& $program @arguments;if($LASTEXITCODE -ne 0){throw "步骤失败：$program ($LASTEXITCODE)"}}
function Sign-Verified([string]$path){
  if($signing){
    $null=Set-AuthenticodeSignature -FilePath $path -Certificate $certificate -TimestampServer $signing.timestampUrl -HashAlgorithm SHA256
    $result=Get-AuthenticodeSignature -FilePath $path
    if($result.Status -ne 'Valid' -or $result.SignerCertificate.Subject -ne $signing.expectedSubject){throw '发行签名或身份核验失败。'}
  } elseif((Get-AuthenticodeSignature -FilePath $path).Status -ne 'NotSigned'){throw '出现来源不明的签名状态，停止。'}
}
Invoke-Checked 'cargo' @('fmt','--all','--check')
Invoke-Checked 'cargo' @('check','--locked')
Invoke-Checked 'cargo' @('clippy','--all-targets','--locked','--','-D','warnings')
Invoke-Checked 'cargo' @('test','--all-targets','--locked')
Invoke-Checked 'cargo' @('build','--release','--locked')
$metadata=& cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$binary=Join-Path $metadata.target_directory 'release/nanfeng-codex-quota.exe'
Sign-Verified $binary
$output=if([IO.Path]::IsPathRooted($OutDir)){[IO.Path]::GetFullPath($OutDir)}else{[IO.Path]::GetFullPath((Join-Path $root $OutDir))}
New-Item -ItemType Directory -Path $output -Force | Out-Null
Invoke-Checked $Compiler @('/Qp',"/DAppVersion=$version","/DAppBinary=$binary","/DReleaseOutput=$output",'packaging/windows.iss')
$setup=Join-Path $output "Nanfeng-Codex-Quota-Windows-v$version-Setup.exe"
Sign-Verified $setup
Write-Output "安装包：$setup"
Write-Output $(if($signing){'发行签名：已验签'}else{'发行签名：未签名，已明确允许本次构建'})
