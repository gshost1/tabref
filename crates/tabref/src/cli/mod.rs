pub(crate) mod command;

use clap::Parser;

use command::Command;

/// Read and drive Chrome from the shell. Start in one call: `tabref skill && tabref open <url>`.
///
/// `tabref skill` prints the short guide; `tabref open` prints the page with
/// a ref per control (e4) that the actions take, as do names the page shows
/// ("Save"). Commands talk to Chrome over the DevTools protocol.
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
    /// Deadline for the whole command, in milliseconds (default 10000, and
    /// 120000 for `do`)
    #[arg(long, global = true, value_name = "MS")]
    pub(crate) timeout: Option<u64>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

const TIMEOUT_MS: u64 = 10_000;
const DO_TIMEOUT_MS: u64 = 120_000;

impl Cli {
    /// The command's deadline: `--timeout`, else 10 s, or 120 s for `do`.
    pub(crate) fn timeout_ms(&self) -> u64 {
        self.timeout.unwrap_or(match self.command {
            Command::Do { .. } => DO_TIMEOUT_MS,
            _ => TIMEOUT_MS,
        })
    }
}
