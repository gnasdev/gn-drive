//! `gn-drive` CLI — port of `Sources/gn-drive` (Swift ArgumentParser → clap).

mod commands;
mod serve;

use clap::{Args, CommandFactory, Parser, Subcommand};
use gn_drive_core::build_version;

/// Build-time version / commit (port of Swift `BuildInfo`).
pub const VERSION: &str = build_version::CURRENT;
pub const COMMIT: &str = "unknown";

/// Error type shared by all commands; `Display` is printed as `Error: <msg>`.
pub type CliError = Box<dyn std::error::Error>;
pub type CliResult = Result<(), CliError>;

/// Port of Swift `CLIError.app`.
pub fn app_err(msg: impl Into<String>) -> CliError {
    msg.into().into()
}

const ABOUT: &str = "GN Drive — local-only sync engine and native macOS app";

const AFTER_HELP: &str = "\
Subcommands:
  run          Start the engine in foreground or service mode
  serve        Serve the stdio RPC protocol for the GPUI window
  service      Install, uninstall, start, stop, or check service status
  sync         One-shot sync (pull, push, bi, bi-resync, dry-run)
  board        (legacy) Execute a board DAG — prefer flows
  profile      Manage sync profiles
  remote       Manage rclone remotes
  self-update  Download and apply updates from GitHub Releases
  version      Print version info
  doctor       Diagnose environment and configuration";

#[derive(Parser)]
#[command(
    name = "gn-drive",
    about = ABOUT,
    after_help = AFTER_HELP,
    disable_version_flag = true,
    arg_required_else_help = true
)]
struct Cli {
    /// Show the version.
    #[arg(long, global = true)]
    version: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start gn-drive engine in foreground or service mode
    Run(RunArgs),
    /// Serve the newline-delimited JSON RPC protocol on stdio (protocol v1)
    Serve(ServeArgs),
    /// Install, uninstall, start, stop, or check service status
    Service(ServiceArgs),
    /// Run a one-shot sync operation
    Sync(SyncArgs),
    /// Execute a board DAG (legacy)
    #[command(after_help = "Legacy CLI: boards are not part of the app workspace; prefer flows.")]
    Board(BoardArgs),
    /// Manage sync profiles
    Profile {
        #[command(subcommand)]
        command: Option<ProfileCmd>,
    },
    /// Manage rclone remotes
    Remote {
        #[command(subcommand)]
        command: Option<RemoteCmd>,
    },
    /// Download and apply the latest release
    SelfUpdate(SelfUpdateArgs),
    /// Print version info
    Version,
    /// Diagnose environment and configuration
    Doctor(DoctorArgs),
    /// Generate shell completion scripts
    Completion(CompletionArgs),
}

#[derive(Args)]
pub struct RunArgs {
    /// Run as a background service (use 'gn-drive service install' first)
    #[arg(long)]
    pub service: bool,
    /// Development mode (debug-oriented logging)
    #[arg(long)]
    pub dev: bool,
    /// Unlock at process start (service mode / CI). Interactive runs unlock via the app UI instead.
    #[arg(
        long,
        env = "GN_DRIVE_PASSWORD",
        hide_env_values = true,
        default_value = ""
    )]
    pub password: String,
    /// Override the config directory
    #[arg(long, default_value = "")]
    pub config_dir: String,
}

#[derive(Args)]
pub struct ServeArgs {
    /// Required in v1: serve the protocol on stdin/stdout
    #[arg(long)]
    pub stdio: bool,
    /// Override the config directory
    #[arg(long, default_value = "")]
    pub config_dir: String,
    /// Override the rclone binary
    #[arg(long, default_value = "")]
    pub rclone: String,
    /// Minimum log level: debug|info|warn|error
    #[arg(long, default_value = "info")]
    pub log_level: String,
    /// In-memory log ring size for logs.read
    #[arg(long, default_value_t = 2000)]
    pub log_buffer: i64,
    /// Unlock at startup (automation; prefer auth.unlock)
    #[arg(
        long,
        env = "GN_DRIVE_PASSWORD",
        hide_env_values = true,
        default_value = ""
    )]
    pub password: String,
    /// Protocol version (only 1 is supported)
    #[arg(long = "protocol", default_value_t = 1)]
    pub protocol_version: i64,
}

#[derive(Args)]
pub struct ServiceArgs {
    /// install | uninstall | start | stop | status | restart
    pub action: String,
    /// Install as a system-level service (requires sudo)
    #[arg(long)]
    pub system: bool,
}

#[derive(Args)]
pub struct SyncArgs {
    /// pull | push | bi | bi-resync | dry-run
    pub action: String,
    /// Profile name to sync (required)
    #[arg(long)]
    pub profile: String,
    /// Preview only — do not change files
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct BoardArgs {
    /// Board ID
    pub board_id: String,
    /// Stop execution at the first failed edge (default: on; use --no-stop-on-error to disable)
    #[arg(long = "stop-on-error", overrides_with = "no_stop_on_error")]
    pub stop_on_error: bool,
    #[arg(
        long = "no-stop-on-error",
        overrides_with = "stop_on_error",
        hide = true
    )]
    pub no_stop_on_error: bool,
}

#[derive(Subcommand)]
enum ProfileCmd {
    /// List all profiles
    List,
    /// Add a new profile
    Add(ProfileAddArgs),
    /// Delete a profile
    Delete { name: String },
}

#[derive(Args)]
pub struct ProfileAddArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
    #[arg(long, default_value = "push")]
    pub direction: String,
    #[arg(long, default_value_t = 4, allow_negative_numbers = true)]
    pub parallel: i64,
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    pub bandwidth: i64,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Subcommand)]
enum RemoteCmd {
    /// List all remotes
    List,
    /// Add a new remote (non-interactive)
    Add(RemoteAddArgs),
    /// Test a remote connection
    Test { name: String },
    /// Delete a remote
    Delete { name: String },
}

#[derive(Args)]
pub struct RemoteAddArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long = "type")]
    pub r#type: String,
    /// Config k=v pairs (repeatable)
    #[arg(long, num_args = 1.., action = clap::ArgAction::Append)]
    pub config: Vec<String>,
}

#[derive(Args)]
pub struct SelfUpdateArgs {
    #[arg(long, default_value = "")]
    pub repo_owner: String,
    #[arg(long, default_value = "")]
    pub repo: String,
    #[arg(long)]
    pub force: bool,
    #[arg(long = "check")]
    pub check_only: bool,
}

#[derive(Args)]
pub struct DoctorArgs {
    /// List files in config directory
    #[arg(long)]
    pub data: bool,
}

#[derive(Args)]
pub struct CompletionArgs {
    /// bash | zsh | fish
    pub shell: String,
}

/// The clap command tree (used by `completion`).
pub fn command() -> clap::Command {
    Cli::command()
}

fn main() {
    let cli = Cli::parse();
    if cli.version {
        println!("{VERSION}");
        return;
    }
    let Some(cmd) = cli.command else {
        let _ = command().print_help();
        std::process::exit(2);
    };
    let result = match cmd {
        Command::Run(a) => commands::run(a),
        Command::Serve(a) => serve::serve(a),
        Command::Service(a) => commands::service(a),
        Command::Sync(a) => commands::sync(a),
        Command::Board(a) => commands::board(a),
        Command::Profile { command } => match command.unwrap_or(ProfileCmd::List) {
            ProfileCmd::List => commands::profile_list(),
            ProfileCmd::Add(a) => commands::profile_add(a),
            ProfileCmd::Delete { name } => commands::profile_delete(&name),
        },
        Command::Remote { command } => match command.unwrap_or(RemoteCmd::List) {
            RemoteCmd::List => commands::remote_list(),
            RemoteCmd::Add(a) => commands::remote_add(a),
            RemoteCmd::Test { name } => commands::remote_test(&name),
            RemoteCmd::Delete { name } => commands::remote_delete(&name),
        },
        Command::SelfUpdate(a) => commands::self_update(a),
        Command::Version => {
            println!("gn-drive {VERSION} (commit={COMMIT}, rust)");
            Ok(())
        }
        Command::Doctor(a) => commands::doctor(a),
        Command::Completion(a) => commands::completion(a),
    };
    if let Err(e) = result {
        use std::io::Write;
        let _ = std::io::stdout().flush();
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
