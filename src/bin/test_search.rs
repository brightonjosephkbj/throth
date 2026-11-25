fn main() {
    let client = reqwest::blocking::Client::builder().user_agent("Mozilla/5.0").build().unwrap();
    let res = client.get("https://ww1.goojara.to/").send().unwrap();
    let html = res.text().unwrap();
    let mut data_ins = String::new();
    if let Some(idx) = html.find("data-ins=\"") {
        let sub = &html[idx+10..];
        if let Some(end) = sub.find('"') {
            data_ins = sub[..end].to_string();
        }
    }
    println!("data_ins: {}", data_ins);

    let resp = client.post("https://ww1.goojara.to/xmre.php")
        .header("Referer", "https://ww1.goojara.to/")
        .header("Origin", "https://ww1.goojara.to/")
        .header("X-Requested-With", "XMLHttpRequest")
        .body(format!("z={}&x=044ecc0040&q=Inception", data_ins))
        .send().unwrap();
    println!("Status: {}", resp.status());
    let text = resp.text().unwrap();
    println!("Search result: {}", text);
}
