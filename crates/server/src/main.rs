//! Service entry point.

use github_ranked::auth::{self, AuthProvider};
use github_ranked::config::Config;
use github_ranked::error::ApiResult;
use github_ranked::routes::router;
use github_ranked::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// How often expired rows are swept from the durable cache.
const PURGE_INTERVAL: Duration = Duration::from_secs(3_600);

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        // The tracing subscriber may not be up yet, so print as well.
        eprintln!("fatal: {error}");
        tracing::error!(%error, "startup failed");
        std::process::exit(1);
    }
}

async fn run() -> ApiResult<()> {
    let config = Config::from_env()?;
    init_tracing(config.environment);

    tracing::info!(
        environment = ?config.environment,
        bind = %config.bind,
        web_root = %config.web_root.display(),
        "starting github-ranked"
    );

    let auth = build_auth_provider(&config)?;

    let state = AppState::new(config.clone(), auth)?;

    // Sweep expired cache rows in the background. Reads already ignore expired
    // entries, so this is purely about not growing the database forever.
    tokio::spawn({
        let state = state.clone();
        async move {
            let mut ticker = tokio::time::interval(PURGE_INTERVAL);
            // The first tick fires immediately; skip it so startup stays quick.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                state.purge_cache().await;
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .map_err(|e| {
            github_ranked::error::ApiError::Internal(format!("binding {}: {e}", config.bind))
        })?;

    tracing::info!(address = %config.bind, "listening");

    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| github_ranked::error::ApiError::Internal(format!("server error: {e}")))?;

    tracing::info!("shutdown complete");
    Ok(())
}

/// Select an authentication provider for this build.
///
/// Which providers exist is a compile-time property. Only the PAT provider is
/// implemented so far; the GitHub App providers will slot in here as additional
/// arms without touching anything downstream of `AuthProvider`.
fn build_auth_provider(
    config: &github_ranked::config::Config,
) -> ApiResult<Arc<dyn AuthProvider>> {
    if !auth::any_provider_available() {
        return Err(github_ranked::error::ApiError::Internal(
            "this build has no authentication provider compiled in; rebuild with \
             --features pat-auth"
                .into(),
        ));
    }

    #[cfg(feature = "pat-auth")]
    {
        Ok(Arc::new(auth::PatProvider::from_env(
            config.environment.is_production(),
        )?))
    }

    // Unreachable while `pat-auth` is the only provider, but keeps the function
    // total so adding the App provider is a local change.
    #[cfg(not(feature = "pat-auth"))]
    {
        let _ = config;
        unreachable!("guarded by any_provider_available")
    }
}

fn init_tracing(environment: github_ranked::config::Environment) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,github_ranked=debug"));

    let registry = tracing_subscriber::registry().with(filter);

    // Structured logs in production for ingestion; readable ones locally.
    if environment.is_production() {
        registry.with(tracing_subscriber::fmt::layer().json()).init();
    } else {
        registry.with(tracing_subscriber::fmt::layer()).init();
    }
}

/// Wait for SIGTERM or Ctrl-C.
///
/// Kubernetes sends SIGTERM and then waits before SIGKILL; handling it lets
/// in-flight badge renders finish instead of being cut off mid-rollout.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "cannot listen for SIGTERM"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received interrupt"),
        _ = terminate => tracing::info!("received SIGTERM"),
    }

    tracing::info!("draining in-flight requests");
}
