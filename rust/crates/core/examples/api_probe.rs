use douyin_core::api::DouyinApi;

fn main() {
    let aweme_id = std::env::args().nth(1).expect("aweme_id");
    let api = DouyinApi::new().unwrap();
    match api.fetch_aweme_detail(&aweme_id) {
        Some(detail) => println!("{detail:#?}"),
        None => println!("fetch failed"),
    }
}
