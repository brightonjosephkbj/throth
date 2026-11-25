use super::provider::{MediaType, ProviderError, SearchProvider, SearchResult};
use crate::engine::cdp::extract_wootly_via_cdp;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;

pub struct GoojaraProvider {}

impl GoojaraProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {})
    }

    pub fn extract_stream(&self, movie_url: &str) -> Result<String, ProviderError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(extract_wootly_via_cdp(movie_url)).map_err(|e| e.to_string())
    }

    pub fn popular(&self) -> Result<Vec<SearchResult>, ProviderError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(fetch_popular_goojara_via_cdp()).map_err(|e| e.to_string())
    }
}

async fn fetch_popular_goojara_via_cdp() -> Result<Vec<SearchResult>, String> {
    crate::engine::cdp::ensure_chromium_running().await?;

    let client = reqwest::Client::new();
    let resp = client.get("http://127.0.0.1:9222/json/list")
        .send()
        .await
        .map_err(|e| format!("Failed to list targets: {}", e))?;
    
    let targets: Vec<Value> = resp.json()
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
        let new_resp = client.put(&format!("http://127.0.0.1:9222/json/new?{}", urlencoding::encode("https://ww1.goojara.to/watch-trends-popular")))
            .send()
            .await
            .map_err(|e| format!("Failed to create tab: {}", e))?;
        let body_text = new_resp.text().await
            .map_err(|e| format!("Failed to read new target response body: {}", e))?;
        let new_target: Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Failed to parse new target JSON ('{}'): {}", body_text, e))?;
        ws_url = new_target["webSocketDebuggerUrl"].as_str().ok_or("Missing ws url")?.to_string();
    }

    let (ws_stream, _) = connect_async(&ws_url).await
        .map_err(|e| format!("Failed to connect to CDP websocket: {}", e))?;

    let mut msg_id = 0;

    async fn send_command(ws: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, tokio_tungstenite::tungstenite::Message>, id: &mut i32, method: &str, params: Value) -> Result<i32, String> {
        *id += 1;
        let cmd = json!({
            "id": *id,
            "method": method,
            "params": params
        });
        ws.send(tokio_tungstenite::tungstenite::Message::Text(cmd.to_string().into())).await
            .map_err(|e| format!("WS send error: {}", e))?;
        Ok(*id)
    }

    let (mut write, mut read) = ws_stream.split();

    send_command(&mut write, &mut msg_id, "Runtime.enable", json!({})).await?;
    send_command(&mut write, &mut msg_id, "Page.enable", json!({})).await?;
    send_command(&mut write, &mut msg_id, "Page.navigate", json!({"url": "https://ww1.goojara.to/watch-trends-popular"})).await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let expression = r##"
        (async () => {
            const deadline = Date.now() + 12000;

            while (Date.now() < deadline) {
                const results = [...document.querySelectorAll(".mfeed a[href]")]
                    .filter(a => {
                        const movie = a.querySelector(".im");
                        return movie && !a.querySelector(".it");
                    })
                    .map(a => {
                        const movie = a.querySelector(".im");
                        const img = a.querySelector("img");
                        const title = (movie.innerText || movie.textContent || "")
                            .trim()
                            .replace(/\s+/g, " ")
                            .replace(/\s+(DVD|HDTV|HDR)$/i, "");

                        return {
                            title,
                            url: a.href,
                            poster_url: img
                                ? (img.getAttribute("src") ||
                                   img.getAttribute("data-src") ||
                                   img.getAttribute("data-lazy-src"))
                                : null
                        };
                    })
                    .filter(item =>
                        item.title &&
                        item.url &&
                        item.url.includes("goojara.to")
                    );

                if (results.length) return results;
                await new Promise(resolve => setTimeout(resolve, 250));
            }

            return [];
        })()
    "##;

    let fetch_eval = json!({
        "expression": expression,
        "returnByValue": true,
        "awaitPromise": true
    });

    let command_id = send_command(
        &mut write,
        &mut msg_id,
        "Runtime.evaluate",
        fetch_eval
    ).await?;

    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(12));
    tokio::pin!(timeout);

    let mut results = Vec::new();

    loop {
        tokio::select! {
            _ = &mut timeout => break,
            msg = read.next() => {
                if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(txt))) = msg {
                    if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                        if v["id"].as_i64() == Some(command_id as i64) {
                            if let Some(arr) = v["result"]["result"]["value"].as_array() {
                                for item in arr {
                                    if let (Some(title), Some(url)) = (item["title"].as_str(), item["url"].as_str()) {
                                        let upper = title.to_uppercase();
                                        if !upper.contains("WOOTLY") && !upper.contains("LULUVDO") && !upper.contains("VIDSRC") {
                                            let poster_url = item["poster_url"].as_str().map(|s| s.to_string());
                                            results.push(SearchResult {
                                                title: title.to_string(),
                                                creator: None,
                                                url: url.to_string(),
                                                provider: "Goojara".to_string(),
                                                artwork_url: poster_url,
                                                duration_ms: None,
                                                media_type: MediaType::Video,
                                            });
                                        }
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}

async fn search_goojara_via_cdp(query: &str) -> Result<Vec<SearchResult>, String> {
    crate::engine::cdp::ensure_chromium_running().await?;

    let client = reqwest::Client::new();
    let resp = client.get("http://127.0.0.1:9222/json/list")
        .send()
        .await
        .map_err(|e| format!("Failed to list targets: {}", e))?;
    
    let targets: Vec<Value> = resp.json()
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
        let new_resp = client.put(&format!("http://127.0.0.1:9222/json/new?{}", urlencoding::encode("https://ww1.goojara.to/")))
            .send()
            .await
            .map_err(|e| format!("Failed to create tab: {}", e))?;
        let body_text = new_resp.text().await
            .map_err(|e| format!("Failed to read new target response body: {}", e))?;
        let new_target: Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Failed to parse new target JSON ('{}'): {}", body_text, e))?;
        ws_url = new_target["webSocketDebuggerUrl"].as_str().ok_or("Missing ws url")?.to_string();
    }

    let (ws_stream, _) = connect_async(&ws_url).await
        .map_err(|e| format!("Failed to connect to CDP websocket: {}", e))?;

    let mut msg_id = 0;

    async fn send_command(ws: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, tokio_tungstenite::tungstenite::Message>, id: &mut i32, method: &str, params: Value) -> Result<i32, String> {
        *id += 1;
        let cmd = json!({
            "id": *id,
            "method": method,
            "params": params
        });
        ws.send(tokio_tungstenite::tungstenite::Message::Text(cmd.to_string().into())).await
            .map_err(|e| format!("WS send error: {}", e))?;
        Ok(*id)
    }

    let (mut write, mut read) = ws_stream.split();

    send_command(&mut write, &mut msg_id, "Runtime.enable", json!({})).await?;
    send_command(&mut write, &mut msg_id, "Page.enable", json!({})).await?;
    send_command(&mut write, &mut msg_id, "Page.navigate", json!({"url": "https://ww1.goojara.to/"})).await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;


    // Trigger Goojara's real search input handler inside Chromium.
    let query_js = serde_json::to_string(query)
        .map_err(|e| format!("Failed to encode query: {}", e))?;

    let expression = format!(r##"
        (async () => {{
            const input = document.querySelector("#shd");
            const box = document.querySelector("#res");

            if (!input || !box) {{
                throw new Error("Goojara search controls not found");
            }}

            box.innerHTML = "";
            input.value = {query};
            input.dispatchEvent(new Event("input", {{ bubbles: true }}));

            const deadline = Date.now() + 12000;

            while (Date.now() < deadline) {{
                const items = [...box.querySelectorAll(".mfeed li a[href]")].filter(a => a.querySelector(".im"));

                const results = items.map(a => {{
                    const im = a.querySelector(".im");
                    let posterUrl = null;
                    if (im) {{
                        const img = im.querySelector("img");
                        if (img) {{
                            posterUrl = img.getAttribute("src") || img.getAttribute("data-src") || img.getAttribute("data-lazy-src") || img.getAttribute("srcset");
                        }}
                    }}
                    return {{
                        title: (a.innerText || a.textContent || "")
                            .trim().replace(/\\s+/g, " "),
                        url: a.href,
                        poster_url: posterUrl
                    }};
                }}).filter(item => item.title && item.url);

                if (results.length) return results;

                await new Promise(resolve => setTimeout(resolve, 150));
            }}

            return [];
        }})()
    "##, query = query_js);

    let fetch_eval = json!({
        "expression": expression,
        "returnByValue": true,
        "awaitPromise": true
    });

    let command_id = send_command(
        &mut write,
        &mut msg_id,
        "Runtime.evaluate",
        fetch_eval
    ).await?;

    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(12));
    tokio::pin!(timeout);

    let mut results = Vec::new();

    loop {
        tokio::select! {
            _ = &mut timeout => break,
            msg = read.next() => {
                if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(txt))) = msg {
                    if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                        if v["id"].as_i64() == Some(command_id as i64) {
                            if let Some(arr) = v["result"]["result"]["value"].as_array() {
                                for item in arr {
                                    if let (Some(title), Some(url)) = (item["title"].as_str(), item["url"].as_str()) {
                                        let upper = title.to_uppercase();
                                        if !upper.contains("WOOTLY") && !upper.contains("LULUVDO") && !upper.contains("VIDSRC") {
                                            let poster_url = item["poster_url"].as_str().map(|s| s.to_string());
                                            results.push(SearchResult {
                                                title: title.to_string(),
                                                creator: None,
                                                url: url.to_string(),
                                                provider: "Goojara".to_string(),
                                                artwork_url: poster_url,
                                                duration_ms: None,
                                                media_type: MediaType::Video,
                                            });
                                        }
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    if !results.is_empty() {
        let q_lower = query.to_lowercase();
        let q_terms: Vec<String> = q_lower
            .split_whitespace()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut indexed: Vec<(usize, SearchResult)> = results.into_iter().enumerate().collect();
        indexed.sort_by(|a, b| {
            let t_a = a.1.title.to_lowercase();
            let t_b = b.1.title.to_lowercase();

            // 1) exact title matches first
            let exact_a = t_a == q_lower;
            let exact_b = t_b == q_lower;
            if exact_a != exact_b {
                return exact_b.cmp(&exact_a);
            }

            // 2) titles beginning with the query next
            let starts_a = t_a.starts_with(&q_lower);
            let starts_b = t_b.starts_with(&q_lower);
            if starts_a != starts_b {
                return starts_b.cmp(&starts_a);
            }

            // 3) titles containing the full query next
            let contains_a = t_a.contains(&q_lower);
            let contains_b = t_b.contains(&q_lower);
            if contains_a != contains_b {
                return contains_b.cmp(&contains_a);
            }

            // 4) titles matching the most query terms
            if !q_terms.is_empty() {
                let matches_a = q_terms.iter().filter(|term| t_a.contains(*term)).count();
                let matches_b = q_terms.iter().filter(|term| t_b.contains(*term)).count();
                if matches_a != matches_b {
                    return matches_b.cmp(&matches_a);
                }
            }

            // 5) preserve Goojara's original order when scores are equal
            a.0.cmp(&b.0)
        });

        results = indexed.into_iter().map(|item| item.1).collect();
    }

    Ok(results)
}

impl SearchProvider for GoojaraProvider {
    fn name(&self) -> &'static str {
        "Goojara"
    }

    fn search(&self, query: &str) -> Result<Vec<SearchResult>, ProviderError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(search_goojara_via_cdp(query)).map_err(|e| e.to_string())
    }
}
