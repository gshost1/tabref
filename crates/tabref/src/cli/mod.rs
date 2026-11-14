pub(crate) mod command;

use clap::Parser;

use command::Command;

/// Read and drive Chrome from the command line over the DevTools protocol.
/// Every element in a snapshot gets a ref (@s8f3k2p9:e4) that the action
/// commands take.
#[derive(Debug, Parser)]
#[command(name = "tabref", version, max_term_width = 100)]
pub(crate) struct Cli {
    /// DevTools endpoint: a port, http://127.0.0.1:PORT or the browser's ws:// URL
    #[arg(long, global = true, env = "TABREF_CDP", value_name = "ENDPOINT")]
    pub(crate) cdp: Option<String>,
    /// Tab to act in: its index in 'tabs' or its target id
    #[arg(long, global = true, value_name = "TAB")]
    pub(crate) tab: Option<String>,
    /// Print the JSON envelope {version, ok, command, data|error}
    #[arg(long, global = true)]
    pub(crate) json: bool,
    /// Deadline for the whole command, in milliseconds
    #[arg(long, global = true, default_value_t = 10_000, value_name = "MS")]
    pub(crate) timeout: u64,
    #[command(subcommand)]
    pub(crate) command: Command,
}
