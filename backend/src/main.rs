use std::net::SocketAddr;

use quorum_server::config::Config;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "Usage: quorum-server [serve] [--demo] | openapi

  serve     Run the HTTP server (default)
  --demo    Run with fake data for every module and an in-memory database
  openapi   Print the OpenAPI document as JSON to stdout

Environment:
  QUORUM_CONFIG      Config file (default /etc/quorum/quorum.toml)
  QUORUM_SECRET_KEY  32-byte base64 master key for stored secrets (required unless --demo)
  QUORUM_BIND, QUORUM_DATABASE, QUORUM_PUBLIC_URL  override the config file
  RUST_LOG           Log filter (default: info)";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let demo = args.iter().any(|a| a == "--demo");
    let command = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(String::as_str);
    match command {
        None | Some("serve") if !args.iter().any(|a| a == "-h" || a == "--help") => {
            serve(demo).await
        }
        Some("openapi") => {
            let spec = quorum_server::api::openapi();
            println!("{}", spec.to_pretty_json()?);
            Ok(())
        }
        None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => {
            eprintln!("Unknown command: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}

async fn serve(demo: bool) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::load()?;
    let bind = config.server.bind;
    let state = quorum_server::app::build(config, demo).await?;
    quorum_server::app::spawn_maintenance(state.clone());

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, version = quorum_server::VERSION, demo, "{} server listening", quorum_server::APP_NAME);
    axum::serve(
        listener,
        quorum_server::api::router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
