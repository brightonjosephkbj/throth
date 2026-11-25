fn main() {
    let client = reqwest::blocking::Client::builder().user_agent("Mozilla/5.0").build().unwrap();
    let resp = client.get("https://ww1.goojara.to/mKZNZ7")
        .header("Referer", "https://ww1.goojara.to/")
        .send().unwrap();
    println!("Status: {}", resp.status());
    let html = resp.text().unwrap();
    println!("HTML len: {}", html.len());
    for line in html.lines() {
        if line.contains("wootly") || line.contains("iframe") || line.contains("go.php") {
            println!("MATCH: {}", line);
        }
    }
}
