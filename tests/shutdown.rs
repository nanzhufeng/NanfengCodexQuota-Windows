#![cfg(windows)]
use nanfeng_codex_quota::{
    config::Config,
    worker::{Event, Worker},
};
use std::{
    fs,
    time::{Duration, Instant},
};
#[test]
fn exit_cancels_an_in_flight_cli_and_reaps_it() {
    let dir = tempfile::tempdir().unwrap();
    let cli = dir.path().join("waiting.ps1");
    let marker = dir.path().join("pid.txt");
    fs::write(
        &cli,
        format!(
            "[IO.File]::WriteAllText('{}', $PID.ToString())\nStart-Sleep -Seconds 60",
            marker.display().to_string().replace('\'', "''")
        ),
    )
    .unwrap();
    let (mut worker, events) = Worker::start(
        Config {
            codex_path: Some(cli),
            ..Default::default()
        },
        || {},
    );
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(2)).unwrap(),
        Event::Started
    ));
    let deadline = Instant::now() + Duration::from_secs(4);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(30));
    }
    assert!(marker.exists(), "fixture CLI was not launched");
    let pid: u32 = fs::read_to_string(marker).unwrap().parse().unwrap();
    unsafe {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, WaitForSingleObject},
        };
        let handle = OpenProcess(0x00100000, 0, pid);
        assert!(!handle.is_null());
        let started = Instant::now();
        worker.stop();
        let elapsed = started.elapsed();
        let exit = WaitForSingleObject(handle, 1000);
        CloseHandle(handle);
        assert!(
            elapsed < Duration::from_secs(2),
            "shutdown blocked for {elapsed:?}"
        );
        assert_eq!(exit, 0, "CLI was left running");
    }
}
