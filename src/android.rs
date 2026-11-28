use std::panic::{catch_unwind, AssertUnwindSafe};

use jni::objects::{JObject, JString};
use jni::sys::jstring;
use jni::JNIEnv;

fn return_json(env: &mut JNIEnv, json: String) -> jstring {
    env.new_string(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn encode_result(result: Result<String, String>) -> String {
    match result {
        Ok(value) => value,
        Err(error) => serde_json::json!({"error": error}).to_string(),
    }
}

fn encode_panic(message: &str) -> String {
    serde_json::json!({"error": message}).to_string()
}

// Existing YouTube search
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_throthmedia_ThrothModule_nativeSearch(
    mut env: JNIEnv,
    _this: JObject,
    query: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let q: String = env
            .get_string(&query)
            .map_err(|e| e.to_string())?
            .into();

        use crate::providers::youtube::YouTubeProvider;
        use crate::providers::SearchProvider;

        let yt = YouTubeProvider::new()
            .map_err(|e| e.to_string())?;

        let results = yt.search(&q)
            .map_err(|e| e.to_string())?;

        serde_json::to_string(&results)
            .map_err(|e| e.to_string())
    }));

    let json = match result {
        Ok(result) => encode_result(result),
        Err(_) => encode_panic("native panic during YouTube search"),
    };

    return_json(&mut env, json)
}

// Goojara movie search with movie details
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_throthmedia_ThrothModule_nativeMovieSearch(
    mut env: JNIEnv,
    _this: JObject,
    query: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let q: String = env
            .get_string(&query)
            .map_err(|e| e.to_string())?
            .into();

        use crate::providers::goojara::GoojaraProvider;
        use crate::providers::movie::{
            MovieDetails,
            fetch_movie_details_via_cdp,
        };
        use crate::providers::SearchProvider;

        let provider = GoojaraProvider::new()
            .map_err(|e| e.to_string())?;

        let results = provider.search(&q)
            .map_err(|e| e.to_string())?;

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| e.to_string())?;

        let details = rt.block_on(async {
            let mut details_list = Vec::new();

            for item in results {
                match fetch_movie_details_via_cdp(&item.url).await {
                    Ok(details) => details_list.push(details),
                    Err(_) => details_list.push(MovieDetails {
                        movie_name: item.title,
                        poster: item.artwork_url,
                        duration: None,
                        link: item.url,
                        cast: Vec::new(),
                    }),
                }
            }

            Ok::<_, String>(details_list)
        })?;

        serde_json::to_string(&details)
            .map_err(|e| e.to_string())
    }));

    let json = match result {
        Ok(result) => encode_result(result),
        Err(_) => encode_panic("native panic during movie search"),
    };

    return_json(&mut env, json)
}

// Resolve a Goojara movie to its direct Wootly stream URL
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_throthmedia_ThrothModule_nativeMovieExtract(
    mut env: JNIEnv,
    _this: JObject,
    movie_url: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let url: String = env
            .get_string(&movie_url)
            .map_err(|e| e.to_string())?
            .into();

        use crate::providers::goojara::GoojaraProvider;

        let provider = GoojaraProvider::new()
            .map_err(|e| e.to_string())?;

        let stream_url = provider.extract_stream(&url)
            .map_err(|e| e.to_string())?;

        serde_json::to_string(&stream_url)
            .map_err(|e| e.to_string())
    }));

    let json = match result {
        Ok(result) => encode_result(result),
        Err(_) => encode_panic("native panic during movie extraction"),
    };

    return_json(&mut env, json)
}

// Goojara popular list. Fast: no per-movie CDP, details load on tap.
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_throthmedia_ThrothModule_nativePopular(
    mut env: JNIEnv,
    _this: JObject,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        use crate::providers::goojara::GoojaraProvider;
        use crate::providers::movie::MovieDetails;

        let provider = GoojaraProvider::new().map_err(|e| e.to_string())?;
        let results = provider.popular().map_err(|e| e.to_string())?;
        let list: Vec<MovieDetails> = results
            .into_iter()
            .map(|item| MovieDetails {
                movie_name: item.title,
                poster: item.artwork_url,
                duration: None,
                link: item.url,
                cast: Vec::new(),
            })
            .collect();
        serde_json::to_string(&list).map_err(|e| e.to_string())
    }));

    let json = match result {
        Ok(result) => encode_result(result),
        Err(_) => encode_panic("native panic during popular"),
    };
    return_json(&mut env, json)
}

// Details (cast, duration, poster) for ONE movie link.
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_throthmedia_ThrothModule_nativeMovieDetails(
    mut env: JNIEnv,
    _this: JObject,
    movie_url: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let url: String = env
            .get_string(&movie_url)
            .map_err(|e| e.to_string())?
            .into();

        use crate::providers::movie::fetch_movie_details_via_cdp;

        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let details = rt
            .block_on(fetch_movie_details_via_cdp(&url))
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&details).map_err(|e| e.to_string())
    }));

    let json = match result {
        Ok(result) => encode_result(result),
        Err(_) => encode_panic("native panic during movie details"),
    };
    return_json(&mut env, json)
}

// SoundCloud search -> JSON array of SearchResult
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_throthmedia_ThrothModule_nativeMusicSearch(
    mut env: JNIEnv,
    _this: JObject,
    query: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let q: String = env.get_string(&query).map_err(|e| e.to_string())?.into();
        use crate::providers::soundcloud::SoundCloudProvider;
        use crate::providers::SearchProvider;
        let provider = SoundCloudProvider::new().map_err(|e| e.to_string())?;
        let results = provider.search(&q).map_err(|e| e.to_string())?;
        serde_json::to_string(&results).map_err(|e| e.to_string())
    }));
    let json = match result {
        Ok(result) => encode_result(result),
        Err(_) => encode_panic("native panic during music search"),
    };
    return_json(&mut env, json)
}

// Resolve one SoundCloud track page URL to playable stream formats
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_throthmedia_ThrothModule_nativeMusicStream(
    mut env: JNIEnv,
    _this: JObject,
    track_url: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let url: String = env.get_string(&track_url).map_err(|e| e.to_string())?.into();
        use crate::engine::resolver::MediaResolver;
        use crate::providers::soundcloud::SoundCloudProvider;
        use crate::providers::{MediaType, SearchResult};

        let provider = SoundCloudProvider::new().map_err(|e| e.to_string())?;
        let item = SearchResult {
            title: String::new(),
            creator: None,
            url,
            provider: "soundcloud".to_string(),
            artwork_url: None,
            duration_ms: None,
            media_type: MediaType::Audio,
        };
        let resolved = provider.resolve(&item)?;
        let formats: Vec<serde_json::Value> = resolved
            .formats
            .iter()
            .map(|f| {
                serde_json::json!({
                    "format_id": f.format_id,
                    "container": f.container,
                    "mime_type": f.mime_type,
                    "direct_url": f.direct_url,
                })
            })
            .collect();
        Ok(serde_json::json!({
            "availability": format!("{:?}", resolved.availability),
            "message": resolved.message,
            "formats": formats,
        })
        .to_string())
    }));
    let json = match result {
        Ok(result) => encode_result(result),
        Err(_) => encode_panic("native panic during music stream"),
    };
    return_json(&mut env, json)
}

// Download one SoundCloud track (clear AAC HLS only) into out_dir.
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_throthmedia_ThrothModule_nativeMusicDownload(
    mut env: JNIEnv,
    _this: JObject,
    track_url: JString,
    title: JString,
    out_dir: JString,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        let url: String = env.get_string(&track_url).map_err(|e| e.to_string())?.into();
        let title: String = env.get_string(&title).map_err(|e| e.to_string())?.into();
        let dir: String = env.get_string(&out_dir).map_err(|e| e.to_string())?.into();

        use crate::engine::downloader::{download_hls_media, inspect_hls_manifest, save_audio_file};
        use crate::engine::resolver::MediaResolver;
        use crate::providers::soundcloud::SoundCloudProvider;
        use crate::providers::{MediaType, SearchResult};

        let provider = SoundCloudProvider::new().map_err(|e| e.to_string())?;
        let item = SearchResult {
            title: String::new(),
            creator: None,
            url,
            provider: "soundcloud".to_string(),
            artwork_url: None,
            duration_ms: None,
            media_type: MediaType::Audio,
        };
        let resolved = provider.resolve(&item)?;

        let mut chosen = None;
        for want in ["aac_160k", "aac_96k"] {
            chosen = resolved.formats.iter().find(|f| {
                f.format_id == want
                    && f.direct_url.is_some()
                    && !f.container.as_deref().unwrap_or("").to_lowercase().contains("encrypted")
            });
            if chosen.is_some() {
                break;
            }
        }
        let fmt = chosen.ok_or("This track is protected (DRM) and can't be downloaded")?;
        let direct = fmt.direct_url.as_ref().ok_or("Format has no direct URL")?;

        let manifest = inspect_hls_manifest(direct)?;
        let protected = manifest
            .encryption_method
            .as_deref()
            .map(|m| m.to_uppercase().contains("SAMPLE"))
            .unwrap_or(false);
        if protected {
            return Err("This track is protected (DRM) and can't be downloaded".to_string());
        }

        let download = download_hls_media(&manifest)?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let name = if title.trim().is_empty() { "track".to_string() } else { title };
        let saved = save_audio_file(&download.data, &dir, &name)?;

        Ok(serde_json::json!({
            "path": saved.to_string_lossy(),
            "format_id": fmt.format_id,
        })
        .to_string())
    }));
    let json = match result {
        Ok(result) => encode_result(result),
        Err(_) => encode_panic("native panic during music download"),
    };
    return_json(&mut env, json)
}
