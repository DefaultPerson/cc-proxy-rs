mod adapter;
mod error;
mod routes;
mod server;
mod subprocess;
mod types;

use std::net::SocketAddr;

use clap::Parser;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "claude-code-proxy")]
#[command(about = "Anthropic Messages API proxy over Claude Code CLI")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "3456")]
    port: u16,

    /// Working directory for the Claude CLI subprocess
    #[arg(long, default_value = ".")]
    cwd: String,

    /// Max agentic turns per request (prevents runaway loops)
    #[arg(long, default_value = "100")]
    max_turns: u32,

    /// Replace Claude Code's system prompt entirely instead of appending
    #[arg(long, default_value = "false")]
    replace_system_prompt: bool,

    /// Effort level for Claude (low, medium, high, max)
    #[arg(long)]
    effort: Option<String>,

    /// Embed system prompt in prompt text instead of using --system-prompt (replace).
    /// Keeps Claude Code's default 43K system prompt intact.
    #[arg(long, default_value = "false")]
    embed_system_prompt: bool,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "claude_code_proxy=info,tower_http=info".parse().unwrap()),
        )
        .compact()
        .with_target(false)
        .init();

    let args = Args::parse();

    // Verify claude CLI is available
    match tokio::process::Command::new("claude")
        .arg("--version")
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            info!("Claude CLI: {}", version.trim());
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("claude --version failed: {stderr}");
            std::process::exit(1);
        }
        Err(e) => {
            error!("claude CLI not found: {e}");
            error!("Install: npm install -g @anthropic-ai/claude-code");
            std::process::exit(1);
        }
    }

    // Resolve cwd
    let cwd = std::fs::canonicalize(&args.cwd)
        .unwrap_or_else(|_| std::path::PathBuf::from(&args.cwd))
        .to_string_lossy()
        .to_string();

    let state = server::AppState {
        cwd: cwd.clone(),
        max_turns: args.max_turns,
        replace_system_prompt: args.replace_system_prompt,
        effort: args.effort,
        embed_system_prompt: args.embed_system_prompt,
    };
    let app = server::create_router(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind {addr}: {e}");
            if e.kind() == std::io::ErrorKind::AddrInUse {
                error!("Port {} is already in use", args.port);
            }
            std::process::exit(1);
        }
    };

    info!("Listening on http://{addr}");
    info!("CWD: {cwd}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| error!("Server error: {e}"));
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("Received Ctrl+C, shutting down"),
        () = terminate => info!("Received SIGTERM, shutting down"),
    }
}
