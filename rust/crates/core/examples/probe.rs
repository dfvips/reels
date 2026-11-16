use douyin_core::extractor::VideoExtractor;

fn shorten(text: &str) -> String {
    text.chars().take(30).collect()
}

fn main() {
    let input = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let cookie = std::env::var("XHS_COOKIE")
        .ok()
        .filter(|c| !c.trim().is_empty());
    let extractor = VideoExtractor::new()
        .expect("client")
        .with_xhs_cookie(cookie);
    for result in extractor.extract_multiple(&input) {
        println!(
            "success={} platform={:?} kind={:?} title={:?}",
            result.success,
            result.platform,
            result.media_kind,
            result.title.as_deref().map(shorten)
        );
        for (i, entry) in result.entries.iter().enumerate() {
            println!(
                "  [{i}] {} size={:?} dur={:?} q={:?} {}x{}",
                entry.extension.as_deref().unwrap_or("mp4"),
                entry.size_bytes,
                entry.duration_secs,
                entry.quality,
                entry
                    .width
                    .map(|w| w.to_string())
                    .unwrap_or_else(|| "-".into()),
                entry
                    .height
                    .map(|h| h.to_string())
                    .unwrap_or_else(|| "-".into()),
            );
            println!("      {}", &entry.url[..entry.url.len().min(90)]);
        }
        if let Some(error) = &result.error {
            println!("  error: {error}");
        }
    }
}
