//! 曲サーバー（reprise の拡張）。使い方は [`reprise_server::USAGE`]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match reprise_server::parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let running = match reprise_server::start(config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let url = format!("http://{}/", running.addr);
    println!("曲サーバーを {url} で始めました");
    println!("合言葉: {}", running.token);
    println!(
        "ページが {url} にあれば {url}#token={} を開くとつながります",
        running.token
    );
    if !running.addr.ip().is_loopback() {
        println!("注意: ほかの機械からもつなげます。譜面・音・画像を配るので、インターネットには公開しないでください");
    }
    running.join();
}
