//! `petruterm-relay --listen 127.0.0.1:8787`
//!
//! Plain ws://. Put a TLS-terminating reverse proxy in front for wss:// when exposing it.
//! The relay only forwards end-to-end encrypted frames and never sees terminal content.

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut listen = "127.0.0.1:8787".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => listen = args.next().unwrap_or(listen),
            "-h" | "--help" => {
                println!("usage: petruterm-relay [--listen ADDR:PORT]");
                return Ok(());
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    log::info!("relay listening on {}", listener.local_addr()?);
    relay::serve(listener, relay::Limits::default()).await
}
