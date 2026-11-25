use throth::providers::goojara::GoojaraProvider;
use throth::providers::SearchProvider;

fn main() {
    let p = GoojaraProvider::new().unwrap();
    for q in &["The Uprising", "spiderman", "Interstellar"] {
        println!("\nQUERY: {}", q);
        let res = p.search(q).unwrap();
        println!("Count: {}", res.len());
        for r in res {
            println!(" - {} -> {}", r.title, r.url);
        }
    }
}
