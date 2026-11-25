fn main() {
    let client = reqwest::blocking::Client::builder().user_agent("Mozilla/5.0").redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let resp = client.get("https://ww1.goojara.to/go.php?i=WjBROWkrNWJ0WVFhNGVRWXp6R013NnJJczNIVUxabWhWYWJZbk9jVUQyQT0")
        .header("Referer", "https://ww1.goojara.to/mKZNZ7")
        .send().unwrap();
    println!("Status: {}", resp.status());
    if let Some(loc) = resp.headers().get("location") {
        println!("Location: {:?}", loc);
    }
}
