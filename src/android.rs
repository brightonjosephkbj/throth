use std::panic::{catch_unwind, AssertUnwindSafe};
use jni::objects::{JObject, JString};
use jni::sys::jstring;
use jni::JNIEnv;

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

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| e.to_string())?;

        rt.block_on(async {
            use crate::providers::youtube::YouTubeProvider;
            use crate::providers::SearchProvider;

            let yt = YouTubeProvider::new()
                .map_err(|e| e.to_string())?;

            let results = yt.search(&q)
                .map_err(|e| e.to_string())?;

            serde_json::to_string(&results)
                .map_err(|e| e.to_string())
        })
    }));

    let json_str = match result {
        Ok(Ok(json)) => json,
        Ok(Err(e)) => serde_json::json!({"error": e}).to_string(),
        Err(_) => serde_json::json!({"error": "native panic during search"}).to_string(),
    };

    env.new_string(json_str)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
