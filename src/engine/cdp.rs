use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use tokio_tungstenite::connect_async;

pub async fn ensure_chromium_running() -> Result<(), String> {
    let client = reqwest::Client::new();
    // Check if CDP port 9222 is already responding
    if client
        .get("http://127.0.0.1:9222/json/version")
        .send()
        .await
        .is_ok()
    {
        return Ok(());
    }

    // Launch headless browser trying multiple executable names in order
    let executables = [
        "google-chrome",
        "google-chrome-stable",
        "chromium-browser",
        "chrome",
    ];
    let mut launched = false;

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let profile_dir = PathBuf::from(home).join("throth/chrome-profile");
    std::fs::create_dir_all(&profile_dir)
        .map_err(|e| format!("Failed to create chrome profile dir: {}", e))?;
    let profile_str = profile_dir.to_string_lossy().to_string();

    for exe in &executables {
        let child = Command::new(exe)
            .arg("--headless=new")
            .arg("--no-sandbox")
            .arg("--disable-gpu")
            .arg("--disable-dev-shm-usage")
            .arg("--remote-debugging-port=9222")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--user-data-dir={}", profile_str))
            .arg("about:blank")
            .spawn();

        if child.is_ok() {
            launched = true;
            break;
        }
    }

    if !launched {
        return Err("Failed to launch any supported browser (tried google-chrome, google-chrome-stable, chromium-browser, chrome). Make sure chrome/chromium is installed.".into());
    }

    // Wait until CDP is ready
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        if client
            .get("http://127.0.0.1:9222/json/version")
            .send()
            .await
            .is_ok()
        {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            return Ok(());
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    }

    Err("Browser launched but CDP port 9222 did not become responsive in time".into())
}

pub async fn extract_wootly_via_cdp(movie_url: &str) -> Result<String, String> {
    ensure_chromium_running().await?;

    let client = reqwest::Client::new();
    let resp = client
        .get("http://127.0.0.1:9222/json/list")
        .send()
        .await
        .map_err(|e| format!("Failed to list targets: {}", e))?;

    let targets: Vec<Value> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse targets: {}", e))?;

    let mut ws_url = String::new();
    for t in &targets {
        let url = t["url"].as_str().unwrap_or("");
        if url.contains("goojara.to") && t["type"].as_str() == Some("page") {
            ws_url = t["webSocketDebuggerUrl"].as_str().unwrap_or("").to_string();
            break;
        }
    }

    if ws_url.is_empty() {
        let new_resp = client
            .put(&format!(
                "http://127.0.0.1:9222/json/new?{}",
                urlencoding::encode(movie_url)
            ))
            .send()
            .await
            .map_err(|e| format!("Failed to create tab: {}", e))?;
        let body_text = new_resp
            .text()
            .await
            .map_err(|e| format!("Failed to read new target response body: {}", e))?;
        let new_target: Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Failed to parse new target JSON ('{}'): {}", body_text, e))?;
        ws_url = new_target["webSocketDebuggerUrl"]
            .as_str()
            .ok_or("Missing ws url")?
            .to_string();
    }

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| format!("Failed to connect to CDP websocket: {}", e))?;

    let mut msg_id = 0;

    async fn send_command(
        ws: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            tokio_tungstenite::tungstenite::Message,
        >,
        id: &mut i32,
        method: &str,
        params: Value,
    ) -> Result<i32, String> {
        *id += 1;
        let cmd = json!({
            "id": *id,
            "method": method,
            "params": params
        });
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            cmd.to_string().into(),
        ))
        .await
        .map_err(|e| format!("WS send error: {}", e))?;
        Ok(*id)
    }

    let (mut write, _read) = ws_stream.split();

    send_command(&mut write, &mut msg_id, "Runtime.enable", json!({})).await?;
    send_command(&mut write, &mut msg_id, "Page.enable", json!({})).await?;
    send_command(
        &mut write,
        &mut msg_id,
        "Page.navigate",
        json!({"url": movie_url}),
    )
    .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;

    // Click provider button
    let eval_cmd = json!({
        "expression": r#"(() => {
            const btn = document.querySelector(".ada[data-host='1']") || document.querySelector(".ada");
            if (btn) {
                btn.removeAttribute("disabled");
                btn.click();
                return "Clicked ada";
            }
            return "No ada";
        })()"#,
        "returnByValue": true
    });

    send_command(&mut write, &mut msg_id, "Runtime.evaluate", eval_cmd).await;

    // Poll for Wootly target with retry/polling mechanism (up to 10 seconds)
    let mut wootly_ws = String::new();
    let poll_deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    let mut target_list_debug = Vec::new();

    while tokio::time::Instant::now() < poll_deadline {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        if let Ok(resp2) = client.get("http://127.0.0.1:9222/json/list").send().await {
            if let Ok(targets2) = resp2.json::<Vec<Value>>().await {
                target_list_debug.clear();
                for t in &targets2 {
                    let u = t["url"].as_str().unwrap_or("");
                    let t_type = t["type"].as_str().unwrap_or("");
                    target_list_debug.push(format!("[{}]: {}", t_type, u));
                    if u.contains("wootly.") || u.contains("/e/") || u.contains("embed") {
                        if let Some(ws) = t["webSocketDebuggerUrl"].as_str() {
                            if !ws.is_empty() {
                                wootly_ws = ws.to_string();
                                break;
                            }
                        }
                    }
                }
            }
        }

        if !wootly_ws.is_empty() {
            break;
        }
    }

    if wootly_ws.is_empty() {
        return Err(format!(
            "Wootly iframe target not found. Active CDP targets: {:?}",
            target_list_debug
        ));
    }

    let (w_stream, _) = connect_async(&wootly_ws)
        .await
        .map_err(|e| format!("Failed to connect to Wootly WS: {}", e))?;
    let (mut w_write, mut w_read) = w_stream.split();

    let mut w_id = 0;
    send_command(&mut w_write, &mut w_id, "Runtime.enable", json!({})).await?;

    // Enable Network domain in Wootly frame to capture requests
    send_command(&mut w_write, &mut w_id, "Network.enable", json!({})).await?;

    // Click prime button
    let w_eval = json!({
        "expression": r#"(() => {
            const prime = document.getElementById("prime");
            if (prime) prime.click();
            if (typeof ply === "function") ply();
            return "Clicked prime";
        })()"#,
        "returnByValue": true
    });

    send_command(&mut w_write, &mut w_id, "Runtime.evaluate", w_eval).await?;

    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(6));
    tokio::pin!(timeout);

    let mut extracted_stream = String::new();

    loop {
        tokio::select! {
            _ = &mut timeout => break,
            msg = w_read.next() => {
                if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(txt))) = msg {
                    if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                        if v["method"].as_str() == Some("Network.requestWillBeSent") {
                            if let Some(url) = v["params"]["request"]["url"].as_str() {
                                if url.contains("source?") || url.contains(".mp4") || url.contains(".m3u8") {
                                    extracted_stream = url.to_string();
                                    break;
                                }
                            }
                        }
                        if v["id"].as_i64() == Some(w_id as i64) {
                            if let Some(res) = v["result"]["result"]["value"].as_str() {
                                if res.starts_with("http") {
                                    extracted_stream = res.to_string();
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if extracted_stream.is_empty() {
        return Err("Failed to capture stream URL from Wootly network requests".into());
    }

    Ok(extracted_stream)
}
