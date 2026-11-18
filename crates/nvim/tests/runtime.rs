//! An editor launching the app must not choose the embedded editor's runtime.
use std::process::Command;

#[test]
fn an_embedded_editor_uses_its_own_runtime_and_accepts_japanese() {
    const PROBE: &str = "TSUMUGI_NVIM_RUNTIME_PROBE";
    if std::env::var_os(PROBE).is_some() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let editor = tsumugi_nvim::Embedded::spawn(
                &tsumugi_nvim::program(),
                &["--clean"],
                80,
                12,
                |_| {},
            )
            .await
            .unwrap();
            editor.type_text("つむぎの入力テスト").await.unwrap();
            assert_eq!(editor.text().await.unwrap(), "つむぎの入力テスト");
            editor.quit().await.unwrap();
        });
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "an_embedded_editor_uses_its_own_runtime_and_accepts_japanese",
            "--nocapture",
        ])
        .env(PROBE, "1")
        .env("TSUMUGI_NVIM", tsumugi_nvim::program())
        .env("VIMRUNTIME", root.path().join("wrong-runtime"))
        .env("VIM", root.path().join("wrong-vim"))
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_STATE_HOME", root.path().join("state"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
