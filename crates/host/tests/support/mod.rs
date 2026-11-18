mod daemon;
pub use daemon::serve_process;

pub fn codex_script(main: &str, developer: Option<&str>) -> String {
    let reply=serde_json::json!({"id":"tsumugi-query","result":{"config":{"developer_instructions":developer}}}).to_string().replace('\'',"'\\''");
    format!(
        r#"if [ "$1" = app-server ]; then
    IFS= read -r initialize
    printf '%s\n' '{{"id":"tsumugi-init","result":{{}}}}'
    while IFS= read -r query; do
        case "$query" in
            *'"method":"config/read"'*) printf '%s\n' '{reply}'; exit 0 ;;
            *'"method":"thread/list"'*) printf '%s\n' "$TSUMUGI_TEST_THREAD_PAGE"; exit 0 ;;
        esac
    done
    exit 1
fi
{main}
"#
    )
}
