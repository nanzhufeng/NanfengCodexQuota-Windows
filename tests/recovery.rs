#![cfg(windows)]
use nanfeng_codex_quota::{
    config::Config,
    monitor::{Failure, Monitor},
    worker::{Command, Event, Worker},
};
use std::{fs, time::Duration};
#[test]
fn failed_query_recovers_on_refresh_without_any_model_request() {
    let dir = tempfile::tempdir().unwrap();
    let cli = dir.path().join("fixture.ps1");
    let ready = dir.path().join("ready");
    let methods = dir.path().join("methods.txt");
    let escape = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
    let script=r#"
$ErrorActionPreference='Stop'
while ($null -ne ($line=[Console]::ReadLine())) {
    $r=$line | ConvertFrom-Json
    [IO.File]::AppendAllText('__METHODS__', $r.method + "`n")
    if ($r.method -eq 'initialize') { $reply=@{id=$r.id;result=@{userAgent='fixture'}} }
    elseif ($r.method -eq 'account/rateLimits/read') {
        if (Test-Path -LiteralPath '__READY__') {
            $reply=@{id=$r.id;result=@{rateLimits=@{primary=@{usedPercent=34;windowDurationMins=10080};secondary=$null;planType='pro'}}}
        } else { $reply=@{id=$r.id;error=@{code=-32000;message='network unavailable'}} }
    } else { throw 'Unexpected non-read-only request' }
    [Console]::WriteLine(($reply | ConvertTo-Json -Depth 8 -Compress))
}
"#.replace("__METHODS__",&escape(&methods)).replace("__READY__",&escape(&ready));
    fs::write(&cli, script).unwrap();
    let (mut worker, events) = Worker::start(
        Config {
            codex_path: Some(cli),
            ..Default::default()
        },
        || {},
    );
    let mut monitor = Monitor::default();
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(3)).unwrap(),
        Event::Started
    ));
    match events.recv_timeout(Duration::from_secs(5)).unwrap() {
        Event::Failed(f) => {
            assert_eq!(f, Failure::Network);
            monitor.failed(f);
        }
        e => panic!("unexpected event {e:?}"),
    };
    fs::write(ready, "ready").unwrap();
    worker.commands.send(Command::Refresh).unwrap();
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(3)).unwrap(),
        Event::Started
    ));
    match events.recv_timeout(Duration::from_secs(5)).unwrap() {
        Event::Success(l, s) => {
            monitor.succeeded(l, s);
        }
        e => panic!("unexpected event {e:?}"),
    };
    assert!(monitor.failure.is_none());
    assert_eq!(
        monitor.render_model(chrono::Utc::now(), 60).percent_label,
        "66%"
    );
    worker.stop();
    for method in fs::read_to_string(methods).unwrap().lines() {
        assert!(["initialize", "account/rateLimits/read"].contains(&method));
    }
}
