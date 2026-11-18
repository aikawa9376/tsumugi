use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

/// Ask an app-server process one question, without creating a thread or turn.
pub(super) async fn request(
    started: crate::Started,
    dir: &Path,
    timeout: Duration,
    method: &str,
    params: Value,
) -> Option<Value> {
    crate::child::converse(started, dir, timeout, async |talk| {
        let initialize = json!({
            "id":"tsumugi-init", "method":"initialize",
            "params":{"clientInfo":{"name":"tsumugi","version":env!("CARGO_PKG_VERSION")}}
        });
        talk.write(&format!("{initialize}\n")).await?;
        talk.until(|line| result(line, "tsumugi-init")).await?;
        let query = json!({"id":"tsumugi-query", "method":method,"params":params});
        talk.write(&format!("{{\"method\":\"initialized\"}}\n{query}\n"))
            .await?;
        talk.until(|line| result(line, "tsumugi-query")).await?
    })
    .await
}

/// The result `line` carries when it answers the request `id`: `Some(None)`
/// for an answer without one.
fn result(line: &str, id: &str) -> Option<Option<Value>> {
    let reply = serde_json::from_str::<Value>(line).ok()?;
    (reply["id"] == id).then(|| reply.get("result").cloned())
}
