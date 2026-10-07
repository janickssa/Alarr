use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;

const USAGE: &str = "Usage: quorum-server [serve | openapi]\n\n  serve    Run the HTTP server (default)\n  openapi  Print the OpenAPI document as JSON to stdout";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        None | Some("serve") => serve().await,
        Some("openapi") => {
            let spec = quorum_server::api::openapi();
            println!("{}", spec.to_pretty_json()?);
            Ok(())
        }
        Some("-h" | "--help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => {
            eprintln!("Unknown command: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // Real config loading (quorum.toml + env) arrives in phase 1.
    let bind: SocketAddr = std::env::var("QUORUM_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_owned())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, version = quorum_server::VERSION, "{} server listening", quorum_server::APP_NAME);
    axum::serve(listener, quorum_server::api::router())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
