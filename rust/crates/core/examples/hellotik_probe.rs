//! hellotik RAW 解析联调探针：
//! `cargo run -p douyin-core --example hellotik_probe [小红书链接]`

fn main() {
    let url = std::env::args().nth(1).unwrap_or_else(|| {
        "https://www.xiaohongshu.com/discovery/item/6aaf3e250000000000b000c43?source=webshare&xhsshare=pc_web&xsec_token=AB3u6itY6beHkCqZuDgLI9MDBqjpg7Eza5NUeZXACiV2l=&xsec_source=pc_share".to_string()
    });
    match douyin_core::hellotik::parse_video(&url) {
        Ok(media) => {
            println!("OK  title = {}", media.title);
            println!("     url   = {}", media.url);
            println!("     ext   = {:?}", media.extension);
        }
        Err(error) => {
            println!("ERR {error}");
            std::process::exit(1);
        }
    }
}
