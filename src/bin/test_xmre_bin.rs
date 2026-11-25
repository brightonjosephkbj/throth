fn main() {
    let client = reqwest::blocking::Client::builder().user_agent("Mozilla/5.0").build().unwrap();
    let resp = client.get("https://ww1.goojara.to/").send().unwrap();
    let headers = resp.headers().clone();
    let home = resp.text().unwrap();
    let mut data_ins = String::new();
    if let Some(idx) = home.find("data-ins=\"") {
        let sub = &home[idx+10..];
        if let Some(end) = sub.find('"') {
            data_ins = sub[..end].to_string();
        }
    }
    println!("data_ins: {}", data_ins);

    let mut cookie_str = String::new();
    for val in headers.get_all("set-cookie") {
        if let Ok(s) = val.to_str() {
            if let Some(semi) = s.find(';') {
                cookie_str.push_str(&format!("{}; ", &s[..semi]));
            } else {
                cookie_str.push_str(&format!("{}; ", s));
            }
        }
    }
    println!("Cookies: {}", cookie_str);

    for q in &["Inception", "Avatar", "Interstellar"] {
        let resp2 = client.post("https://ww1.goojara.to/xmre.php")
            .header("Referer", "https://ww1.goojara.to/")
            .header("Origin", "https://ww1.goojara.to/")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Cookie", cookie_str.clone())
            .body(format!("z={}&x=044ecc0040&q={}", data_ins, q))
            .send().unwrap();
        let status = resp2.status();
        let txt = resp2.text().unwrap();
        println!("Q: {}, Status: {}, Body len: {}, Snippet: {}", q, status, txt.len(), &txt[..std::cmp::min(100, txt.len())]);
    }
}
