//! `doc-shell`: serve endpoints 1 to 7. `doc-shell --print-openapi` prints the OpenAPI document and exits.

use std::sync::Arc;

use doc_acquire::GitAcquirer;
use doc_app::App;
use doc_catalog::HttpCatalog;
use doc_kitchen::KitchenCanon;
use doc_shell::config::Config;
use doc_shell::{build, Shared};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn init_logging(json: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_target(true);
    if json {
        builder.json().init();
    } else {
        builder.init();
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut term = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "cannot install the SIGTERM handler");
            let _ = ctrl_c.await;
            return;
        }
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = term.recv() => {},
    }
    info!("shutdown requested, draining");
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    init_logging(config.log_json);
    let catalog = match HttpCatalog::new(config.catalog.clone()) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "cannot build the HTTP client");
            return std::process::ExitCode::from(2);
        }
    };
    let acquirer = GitAcquirer::new(config.git.clone());
    let shared = Arc::new(Shared {
        app: Arc::new(App::new(catalog, acquirer, Arc::new(KitchenCanon), config.app.clone())),
        default_mode: config.app.default_mode,
    });
    let (router, api) = build(shared, &config.cors_origins);
    if std::env::args().any(|a| a == "--print-openapi") {
        match api.to_pretty_json() {
            Ok(json) => {
                println!("{json}");
                return std::process::ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("cannot serialise the OpenAPI document: {e}");
                return std::process::ExitCode::from(1);
            }
        }
    }
    let listener = match tokio::net::TcpListener::bind(&config.addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(addr = %config.addr, error = %e, "cannot bind");
            return std::process::ExitCode::from(2);
        }
    };
    info!(
        addr = %config.addr, mode = config.app.default_mode.as_str(), data_api = %config.catalog.url,
        cache_dir = %config.git.cache_dir.display(), kitchen = KitchenCanon::KITCHEN_VERSION, "doc-shell listening"
    );
    if let Err(e) = axum::serve(listener, router).with_graceful_shutdown(shutdown_signal()).await {
        error!(error = %e, "server error");
        return std::process::ExitCode::from(1);
    }
    std::process::ExitCode::SUCCESS
}
