use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovieDetails {
    pub movie_name: String,
    pub poster: Option<String>,
    pub duration: Option<String>,
    pub link: String,
    pub cast: Vec<String>,
}

pub async fn fetch_movie_details_via_cdp(url: &str) -> Result<MovieDetails, String> {
    crate::engine::cdp::ensure_chromium_running().await?;

    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:9222/json/list")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let targets: Vec<Value> = response.json().await.map_err(|e| e.to_string())?;

    let mut ws_url = targets.iter()
        .find(|t| t["type"] == "page" &&
            t["url"].as_str().unwrap_or("").contains("goojara.to"))
        .and_then(|t| t["webSocketDebuggerUrl"].as_str())
        .map(str::to_string);

    if ws_url.is_none() {
        let response = client
            .put(format!(
                "http://127.0.0.1:9222/json/new?{}",
                urlencoding::encode(url)
            ))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let target: Value = response.json().await.map_err(|e| e.to_string())?;
        ws_url = target["webSocketDebuggerUrl"].as_str().map(str::to_string);
    }

    let ws_url = ws_url.ok_or("Could not find or create a browser tab")?;
    let (stream, _) = connect_async(&ws_url).await.map_err(|e| e.to_string())?;
    let (mut write, mut read) = stream.split();

    let mut id = 0_i32;

    for (method, params) in [
        ("Runtime.enable", json!({})),
        ("Page.enable", json!({})),
        ("Page.navigate", json!({"url": url})),
    ] {
        id += 1;
        write.send(tokio_tungstenite::tungstenite::Message::Text(
            json!({"id": id, "method": method, "params": params}).to_string().into()
        )).await.map_err(|e| e.to_string())?;
    }

    // Wait until the requested movie page, not an old browser tab, is loaded.
    let target_url = serde_json::to_string(url).map_err(|e| e.to_string())?;
    let readiness_expr = format!(r#"
        (async () => {{
            const targetUrl = {target_url};
            const targetPath = new URL(targetUrl).pathname;
            const deadline = Date.now() + 12000;

            while (Date.now() < deadline) {{
                const text = document.body?.innerText || "";
                const currentPath = location.pathname;

                if (
                    currentPath === targetPath &&
                    document.readyState === "complete" &&
                    text.length > 80
                ) {{
                    return true;
                }}

                await new Promise(r => setTimeout(r, 150));
            }}

            return false;
        }})()
    "#);

    id += 1;
    let ready_cmd_id = id;
    write.send(tokio_tungstenite::tungstenite::Message::Text(
        json!({
            "id": ready_cmd_id,
            "method": "Runtime.evaluate",
            "params": {
                "expression": readiness_expr,
                "returnByValue": true,
                "awaitPromise": true
            }
        }).to_string().into()
    )).await.map_err(|e| e.to_string())?;

    // Quick wait for readiness response or brief fallback
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(13), async {
        while let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) = read.next().await {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                if value["id"].as_i64() == Some(ready_cmd_id as i64) {
                    break;
                }
            }
        }
    }).await;

    let expression = r#"
        (() => {
            const text = document.body?.innerText || "";
            const breadcrumb = text.match(/Movies\s*[→>]\s*([^\n]+)/i);
            const movieName = (breadcrumb?.[1] || document.title || "")
                .replace(/^Watch\s+/i, "")
                .trim();

            const normalized = value => value.toLowerCase()
                .replace(/\s+/g, " ").trim();

            const poster = [...document.images].find(img =>
                img.src.includes("md.goojara.to") &&
                normalized(img.alt) === normalized(movieName)
            )?.src || null;

            const runtimeMatches = [...text.matchAll(/\b(\d{1,4})\s*min(?:ute)?s?\b/gi)];
            const runtime = runtimeMatches.find(match => Number(match[1]) > 0);
            let duration = null;

            if (runtime) {
                const minutes = Number(runtime[1]);
                const hours = Math.floor(minutes / 60);
                const remainder = minutes % 60;
                duration = hours
                    ? (remainder ? `${hours}h ${remainder}m` : `${hours}h`)
                    : `${remainder}m`;
            }

            const castLine = text.match(/Cast:\s*([^\n]+)/i);
            const cast = castLine
                ? castLine[1].split(",").map(name => name.trim()).filter(Boolean)
                : [];

            return {
                movie_name: movieName || null,
                poster,
                duration,
                link: location.href,
                cast
            };
        })()
    "#;

    id += 1;
    let command_id = id;

    write.send(tokio_tungstenite::tungstenite::Message::Text(
        json!({
            "id": command_id,
            "method": "Runtime.evaluate",
            "params": {
                "expression": expression,
                "returnByValue": true
            }
        }).to_string().into()
    )).await.map_err(|e| e.to_string())?;

    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => return Err("Timed out reading movie details".into()),
            message = read.next() => {
                let message = message
                    .ok_or("Browser connection closed")?
                    .map_err(|e| e.to_string())?;

                if let tokio_tungstenite::tungstenite::Message::Text(text) = message {
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|e| e.to_string())?;

                    if value["id"].as_i64() == Some(command_id as i64) {
                        let result = &value["result"]["result"]["value"];

                        if result.is_null() {
                            return Err("Movie detail page returned no data".into());
                        }

                        let details: MovieDetails = serde_json::from_value(result.clone())
                            .map_err(|e| format!("Invalid movie details: {}", e))?;

                        if details.movie_name.is_empty() {
                            return Err("Could not identify the movie title".into());
                        }

                        return Ok(details);
                    }
                }
            }
        }
    }
}
