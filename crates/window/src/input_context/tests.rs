use super::*;
use std::os::unix::fs::PermissionsExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{Duration, timeout};

fn runtime() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    root
}

async fn line(reader: &mut BufReader<UnixStream>) -> String {
    let mut line = String::new();
    timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    line
}

#[test]
fn runtime_discovery_uses_the_user_runtime_independently_of_instances_and_state() {
    assert_eq!(
        runtime_root(Some("/runtime".into()), 1000),
        std::path::PathBuf::from("/runtime")
    );
    assert_eq!(
        runtime_root(None, 1000),
        std::path::PathBuf::from("/run/user/1000")
    );
    assert_eq!(
        runtime_root(Some("".into()), 1000),
        runtime_root(None, 1000)
    );
}

#[tokio::test]
async fn every_input_target_publishes_only_the_version_pid_and_destination() {
    let root = runtime();
    let endpoint = Endpoint::bind(root.path(), 12345).unwrap();
    let path = root.path().join("input-context/12345.sock");
    assert_eq!(endpoint.path, path);
    assert!(!root.path().join("tsumugi").exists());
    let publisher = Publisher::new();
    let server = tokio::spawn(endpoint.serve(publisher.clone()));
    let mut client = BufReader::new(UnixStream::connect(&path).await.unwrap());
    for (target, wire) in [
        (InputTarget::Gui, "gui"),
        (InputTarget::Neovim, "editor"),
        (InputTarget::Terminal, "terminal"),
    ] {
        publisher.update(target);
        assert_eq!(
            line(&mut client).await,
            format!("{{\"version\":1,\"pid\":12345,\"target\":\"{wire}\"}}\n")
        );
    }
    server.abort();
    let _ = server.await;
}

#[test]
fn native_fields_and_a_page_without_focus_publish_gui() {
    for (window, routing_field, want) in [
        (true, true, InputTarget::Neovim),
        (true, false, InputTarget::Gui),
        (false, false, InputTarget::Gui),
        (false, true, InputTarget::Gui),
    ] {
        assert_eq!(
            page_target(InputTarget::Neovim, window, routing_field),
            want
        );
    }
}

#[tokio::test]
async fn a_native_menu_reports_gui_immediately_and_restores_the_latest_route() {
    let publisher = Publisher::new();
    publisher.update(InputTarget::Neovim);
    let (_, mut changes) = publisher.subscribe();
    let menu = publisher.suspend();
    assert_eq!(changes.recv().await.unwrap(), InputTarget::Gui);
    publisher.update(InputTarget::Terminal);
    assert_eq!(publisher.subscribe().0, InputTarget::Gui);
    drop(menu);
    assert_eq!(changes.recv().await.unwrap(), InputTarget::Terminal);
}

#[tokio::test]
async fn a_read_only_subscriber_gets_a_snapshot_changes_and_a_fresh_snapshot_on_reconnect() {
    let root = runtime();
    let endpoint = Endpoint::bind(root.path(), 12345).unwrap();
    let path = endpoint.path.clone();
    let publisher = Publisher::new();
    publisher.update(InputTarget::Neovim);
    let server = tokio::spawn(endpoint.serve(publisher.clone()));
    let mut first = BufReader::new(UnixStream::connect(&path).await.unwrap());
    assert_eq!(
        line(&mut first).await,
        "{\"version\":1,\"pid\":12345,\"target\":\"editor\"}\n"
    );
    publisher.update(InputTarget::Terminal);
    publisher.update(InputTarget::Gui);
    assert!(line(&mut first).await.contains("\"target\":\"terminal\""));
    assert!(line(&mut first).await.contains("\"target\":\"gui\""));
    publisher.update(InputTarget::Gui);
    assert!(
        timeout(Duration::from_millis(30), line(&mut first))
            .await
            .is_err()
    );
    drop(first);
    publisher.update(InputTarget::Terminal);
    let mut again = BufReader::new(UnixStream::connect(&path).await.unwrap());
    assert!(line(&mut again).await.contains("\"target\":\"terminal\""));
    server.abort();
    let _ = server.await;
    assert!(!path.exists());
    assert_eq!(line(&mut again).await, "");
}

#[tokio::test]
async fn a_disconnected_or_idle_subscriber_does_not_hold_up_another() {
    let root = runtime();
    let endpoint = Endpoint::bind(root.path(), 7).unwrap();
    let path = endpoint.path.clone();
    let publisher = Publisher::new();
    let server = tokio::spawn(endpoint.serve(publisher.clone()));
    let idle = UnixStream::connect(&path).await.unwrap();
    let mut active = BufReader::new(UnixStream::connect(&path).await.unwrap());
    assert!(line(&mut active).await.contains("\"target\":\"gui\""));
    for (target, wire) in [
        (InputTarget::Neovim, "editor"),
        (InputTarget::Terminal, "terminal"),
    ] {
        publisher.update(target);
        let got: serde_json::Value = serde_json::from_str(&line(&mut active).await).unwrap();
        assert_eq!(got["target"], wire);
    }
    drop(idle);
    publisher.update(InputTarget::Gui);
    assert!(line(&mut active).await.contains("\"target\":\"gui\""));
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn the_endpoint_is_private_and_its_owner_removes_it() {
    let root = runtime();
    let endpoint = Endpoint::bind(root.path(), std::process::id()).unwrap();
    assert_eq!(
        endpoint.path,
        root.path()
            .join(format!("input-context/{}.sock", std::process::id()))
    );
    for (path, mode) in [
        (root.path().join("input-context"), 0o700),
        (endpoint.path.clone(), 0o600),
    ] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
    let path = endpoint.path.clone();
    assert!(Endpoint::bind(root.path(), std::process::id()).is_err());
    drop(endpoint);
    assert!(!path.exists());
}

#[tokio::test]
async fn unsafe_directories_and_existing_files_are_not_replaced_or_chmodded() {
    let root = runtime();
    let base = root.path().join("input-context");
    std::fs::create_dir(&base).unwrap();
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Endpoint::bind(root.path(), 7).is_err());
    assert_eq!(
        std::fs::metadata(&base).unwrap().permissions().mode() & 0o777,
        0o755
    );
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = base.join("7.sock");
    std::fs::write(&path, "keep").unwrap();
    assert!(Endpoint::bind(root.path(), 7).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "keep");
}

#[tokio::test]
async fn a_subscriber_that_falls_behind_is_disconnected_and_can_reconnect() {
    let root = runtime();
    let endpoint = Endpoint::bind(root.path(), 42).unwrap();
    let path = endpoint.path.clone();
    let publisher = Publisher::new();
    let server = tokio::spawn(endpoint.serve(publisher.clone()));
    let mut client = BufReader::new(UnixStream::connect(&path).await.unwrap());
    assert!(line(&mut client).await.contains("\"target\":\"gui\""));
    // No task yields during the burst: this subscriber falls behind the
    // bounded feed even when the OS socket could still buffer more.
    for _ in 0..100 {
        publisher.update(InputTarget::Neovim);
        publisher.update(InputTarget::Terminal);
    }
    assert_eq!(line(&mut client).await, "");
    let mut again = BufReader::new(UnixStream::connect(&path).await.unwrap());
    assert!(line(&mut again).await.contains("\"target\":\"terminal\""));
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn a_stale_socket_can_be_replaced_but_a_symlink_is_not_followed() {
    let root = runtime();
    let endpoint = Endpoint::bind(root.path(), 7).unwrap();
    let path = endpoint.path.clone();
    drop(endpoint);
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    let endpoint = Endpoint::bind(root.path(), 7).unwrap();
    drop(endpoint);
    let other = root.path().join("keep");
    std::fs::write(&other, "keep").unwrap();
    std::os::unix::fs::symlink(&other, &path).unwrap();
    assert!(Endpoint::bind(root.path(), 7).is_err());
    assert_eq!(std::fs::read_to_string(other).unwrap(), "keep");
    assert!(path.is_symlink());
}
