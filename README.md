# Throth v1.0

Throth is a smart command-line media and movie search utility with Chromium CDP integration for Goojara and Wootly stream extraction.

## Features
- `throth search <query>`: Smart multi-provider search (YouTube, SoundCloud, Goojara).
- `throth movie <query>`: Goojara movie search with poster extraction and ranking.
- `throth popular`: Goojara popular movies trend feed (`/watch-trends-popular`) via Chromium CDP.
- `throth extract <url>`: Direct Wootly stream URL extraction.
- `throth inspect <url>`: HLS manifest inspection.

## Installation & Usage
Build locally using Cargo:
```bash
cargo build --release
```
Run commands:
```bash
throth popular
throth movie "Interstellar"
throth extract <movie_url>
```
