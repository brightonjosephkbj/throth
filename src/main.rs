mod engine;
mod providers;

use engine::downloader::inspect_hls_manifest;
use engine::search::SearchEngine;

use providers::soundcloud::SoundCloudProvider;
use providers::youtube::YouTubeProvider;
use providers::goojara::GoojaraProvider;
use providers::SearchProvider;
use providers::movie::fetch_movie_details_via_cdp;

fn print_usage() {
    println!("Throth CLI Usage:");
    println!("  throth search <query>              - Search across all providers");
    println!("  throth movie [--json] <query> ...  - Search movies using Goojara");
    println!("  throth popular [--json]            - List popular movies from Goojara");
    println!("  throth details <movie_url>         - Extract detailed metadata via CDP");
    println!("  throth extract <movie_url>         - Extract direct stream URL from Goojara/Wootly");
    println!("  throth inspect <url>               - Inspect HLS manifest or media segments");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let command = &args[1];

    let mut ome = SearchEngine::new();
    let soundcloud = SoundCloudProvider::new()?;
    let youtube = YouTubeProvider::new()?;
    let goojara = GoojaraProvider::new()?;
    let goojara_for_movie = GoojaraProvider::new()?;
    let goojara_for_popular = GoojaraProvider::new()?;
    let goojara_for_extract = GoojaraProvider::new()?;

    ome.register_provider(Box::new(soundcloud));
    ome.register_provider(Box::new(youtube));
    ome.register_provider(Box::new(goojara));

    match command.as_str() {
        "search" => {
            let query = args.get(2).cloned().unwrap_or_else(|| "STRUCT slowed".to_string());
            println!("\nOME SMART SEARCH");
            println!("Query: {}", query);
            println!("==============================");

            let response = ome.search(&query);
            println!("Total results: {}", response.results.len());

            for result in &response.results {
                println!("\nTitle: {}", result.title);
                println!("Artist/Channel: {:?}", result.creator);
                println!("Source: {}", result.provider);
                println!("Type: {:?}", result.media_type);
                println!("URL: {}", result.url);
            }
        }
        "movie" => {
            let mut json_mode = false;
            let mut query_parts = Vec::new();

            for arg in args.iter().skip(2) {
                if arg == "--json" {
                    json_mode = true;
                } else {
                    query_parts.push(arg.as_str());
                }
            }

            if query_parts.is_empty() {
                return Err("Missing query argument for movie search".into());
            }
            let query = query_parts.join(" ");

            if !json_mode {
                println!("\nGOOJARA MOVIE SEARCH");
                println!("Query: {}", query);
                println!("==============================");
            }

            let results = goojara_for_movie.search(&query)?;
            let runtime = tokio::runtime::Runtime::new()?;
            let mut details_list = Vec::new();

            for result in results {
                match runtime.block_on(fetch_movie_details_via_cdp(&result.url)) {
                    Ok(details) => {
                        details_list.push(details);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to fetch details for '{}' ({}): {}", result.title, result.url, e);
                        details_list.push(providers::movie::MovieDetails {
                            movie_name: result.title,
                            poster: result.artwork_url,
                            duration: None,
                            link: result.url,
                            cast: Vec::new(),
                        });
                    }
                }
            }

            if json_mode {
                println!("{}", serde_json::to_string_pretty(&details_list)?);
            } else {
                for details in &details_list {
                    println!("\nTitle: {}", details.movie_name);
                    println!("URL: {}", details.link);
                    if let Some(poster) = &details.poster {
                        println!("Poster: {}", poster);
                    }
                    if let Some(duration) = &details.duration {
                        println!("Duration: {}", duration);
                    }
                    if !details.cast.is_empty() {
                        println!("Cast: {}", details.cast.join(", "));
                    }
                }
            }
        }
        "popular" => {
            let json_mode = args.iter().skip(2).any(|arg| arg == "--json");
            let results = goojara_for_popular.popular()?;

            if !json_mode {
                println!("\nGOOJARA POPULAR MOVIES");
                println!("==============================");
                println!("Total popular results: {}", results.len());
            }

            let runtime = tokio::runtime::Runtime::new()?;
            let mut details_list = Vec::new();

            for result in results {
                match runtime.block_on(fetch_movie_details_via_cdp(&result.url)) {
                    Ok(details) => {
                        details_list.push(details);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to fetch details for '{}' ({}): {}", result.title, result.url, e);
                        details_list.push(providers::movie::MovieDetails {
                            movie_name: result.title,
                            poster: result.artwork_url,
                            duration: None,
                            link: result.url,
                            cast: Vec::new(),
                        });
                    }
                }
            }

            if json_mode {
                println!("{}", serde_json::to_string_pretty(&details_list)?);
            } else {
                for details in &details_list {
                    println!("\nTitle: {}", details.movie_name);
                    println!("URL: {}", details.link);
                    if let Some(poster) = &details.poster {
                        println!("Poster: {}", poster);
                    }
                    if let Some(duration) = &details.duration {
                        println!("Duration: {}", duration);
                    }
                    if !details.cast.is_empty() {
                        println!("Cast: {}", details.cast.join(", "));
                    }
                }
            }
        }
        "extract" => {
            let url = args.get(2).ok_or("Missing movie URL argument for extract")?;
            println!("\nEXTRACTING STREAM FROM: {}", url);
            println!("==============================");
            match goojara_for_extract.extract_stream(url) {
                Ok(stream_url) => {
                    println!("SUCCESSFULLY EXTRACTED STREAM URL:\n{}", stream_url);
                }
                Err(e) => {
                    println!("Extraction error: {}", e);
                }
            }
        }
        "details" => {
            let url = args.get(2).ok_or("Missing movie URL argument for details")?;
            let runtime = tokio::runtime::Runtime::new()?;

            match runtime.block_on(fetch_movie_details_via_cdp(url)) {
                Ok(details) => {
                    println!("{}", serde_json::to_string_pretty(&details)?);
                }
                Err(error) => {
                    eprintln!("Details error: {}", error);
                }
            }
        }
        "inspect" => {
            let url = args.get(2).ok_or("Missing URL argument for inspect")?;
            println!("Inspecting media at: {}", url);
            match inspect_hls_manifest(url) {
                Ok(manifest) => {
                    println!("Valid HLS manifest: Yes");
                    println!("Segments count: {}", manifest.segment_urls.len());
                }
                Err(e) => {
                    println!("Inspection error: {}", e);
                }
            }
        }
        _ => {
            print_usage();
        }
    }

    Ok(())
}
