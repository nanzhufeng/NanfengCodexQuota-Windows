#![cfg(windows)]
use nanfeng_codex_quota::{monitor::retry_delay, provider::Provider};
use std::fs;
#[test]
fn transient_network_failure_recovers_within_the_same_read_only_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let cli = dir.path().join("fixture.ps1");
    let marker = dir.path().join("attempted");
    let methods = dir.path().join("methods.txt");
    let escape = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
    let script = r#"
$ErrorActionPreference='Stop'
while ($null -ne ($line=[Console]::ReadLine())) {
 $r=$line | ConvertFrom-Json
 [IO.File]::AppendAllText('__METHODS__', $r.method + "`n")
 if ($r.method -eq 'initialize') { $reply=@{id=$r.id;result=@{userAgent='fixture'}} }
 elseif ($r.method -eq 'account/rateLimits/read') {
  if (Test-Path -LiteralPath '__MARKER__') {
   $reply=@{id=$r.id;result=@{rateLimits=@{primary=@{usedPercent=41;windowDurationMins=10080};secondary=$null;planType='pro'}}}
  } else {
   [IO.File]::WriteAllText('__MARKER__','attempted')
   $reply=@{id=$r.id;error=@{code=-32000;message='network connection temporarily unavailable'}}
  }
 } else { throw 'Unexpected non-read-only request' }
 [Console]::WriteLine(($reply | ConvertTo-Json -Depth 8 -Compress))
}
"#.replace("__METHODS__", &escape(&methods)).replace("__MARKER__", &escape(&marker));
    fs::write(&cli, script).unwrap();
    let (limits, _) = Provider::new(Some(cli))
        .read()
        .expect("one transient network error should recover automatically");
    assert_eq!(limits.effective_primary().remaining_percent(), Some(59));
    let calls = fs::read_to_string(methods).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|m| *m == "account/rateLimits/read")
            .count(),
        2
    );
    assert!(
        calls
            .lines()
            .all(|m| ["initialize", "account/rateLimits/read"].contains(&m))
    );
}
#[test]
fn automatic_recovery_starts_quickly_and_remains_bounded() {
    assert_eq!(retry_delay(60, 0), 60);
    assert_eq!(retry_delay(60, 1), 5);
    assert_eq!(retry_delay(60, 2), 15);
    assert_eq!(retry_delay(60, 3), 30);
    assert_eq!(retry_delay(60, 4), 60);
    assert_eq!(retry_delay(60, 20), 300);
}

#[test]
fn persistent_transport_errors_are_bounded_and_auth_errors_are_not_retried() {
    use nanfeng_codex_quota::monitor::Failure;
    for (message, expected, reads) in [
        ("network unavailable", Failure::Network, 2),
        ("401 Unauthorized", Failure::NotAuthenticated, 1),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("fixture.ps1");
        let methods = dir.path().join("methods.txt");
        let script = r#"
while ($null -ne ($line=[Console]::ReadLine())) {
 $r=$line | ConvertFrom-Json
 if ($r.method -eq 'initialize') { $reply=@{id=$r.id;result=@{userAgent='fixture'}} }
 elseif ($r.method -eq 'account/rateLimits/read') {
  [IO.File]::AppendAllText('__METHODS__', "read`n")
  $reply=@{id=$r.id;error=@{code=-32000;message='__ERROR__'}}
 } else { throw 'Unexpected non-read-only request' }
 [Console]::WriteLine(($reply | ConvertTo-Json -Depth 8 -Compress))
}
"#
        .replace(
            "__METHODS__",
            &methods.display().to_string().replace('\'', "''"),
        )
        .replace("__ERROR__", message);
        fs::write(&cli, script).unwrap();
        assert_eq!(Provider::new(Some(cli)).read().unwrap_err(), expected);
        assert_eq!(fs::read_to_string(methods).unwrap().lines().count(), reads);
    }
}
