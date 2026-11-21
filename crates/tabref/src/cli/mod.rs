pub(crate) mod command;
mod qualifiers;

#[cfg(test)]
mod tests;

use clap::Parser;
use tabref_cdp::DialogChoice;

use command::Command;

/// Read and drive Chrome from the shell. Start in one call: `tabref skill && tabref open <url>`.
///
/// `tabref skill` prints the short guide. `tabref open` prints the page with a
/// ref (e4) on each control; actions take a ref or a name the page shows
/// ("Save"). Commands talk to Chrome over the DevTools protocol.
#[derive(Debug, Parser)]
#[command(name = "tabref", version, max_term_width = 100)]
pub(crate) struct ParsedCli {
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
    /// How to answer the first confirm, prompt or leave-page dialog of the
    /// whole command, a `do` batch included: accept, dismiss, or
    /// accept=TEXT (for the first prompt). Alerts, and every other dialog,
    /// are accepted, as without it; so is a dialog that opens after a
    /// command ends, which the next command reports. Without the daemon
    /// (always on Windows) such a dialog blocks the page until it is
    /// answered in the browser
    #[arg(long, global = true, value_name = "ANSWER", value_parser = dialog_choice)]
    pub(crate) dialog: Option<DialogChoice>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

const TIMEOUT_MS: u64 = 10_000;
const DO_TIMEOUT_MS: u64 = 120_000;

impl ParsedCli {
    /// The command's deadline: `--timeout`, else 10 s, or 120 s for `do`.
    pub(crate) fn timeout_ms(&self) -> u64 {
        self.timeout.unwrap_or(match self.command {
            Command::Do { .. } => DO_TIMEOUT_MS,
            _ => TIMEOUT_MS,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Cli(ParsedCli);

impl std::ops::Deref for Cli {
    type Target = ParsedCli;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Cli {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl clap::CommandFactory for Cli {
    fn command() -> clap::Command {
        qualifiers::command(ParsedCli::command())
    }

    fn command_for_update() -> clap::Command {
        qualifiers::command(ParsedCli::command_for_update())
    }
}

impl clap::FromArgMatches for Cli {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        let normalized = qualifiers::target(matches)?;
        let mut cli = ParsedCli::from_arg_matches(matches)?;
        qualifiers::apply(&mut cli.command, normalized)?;
        Ok(Self(cli))
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        let normalized = qualifiers::target(matches)?;
        self.0.update_from_arg_matches(matches)?;
        qualifiers::apply(&mut self.command, normalized)
    }
}

impl Parser for Cli {}

fn dialog_choice(text: &str) -> Result<DialogChoice, String> {
    DialogChoice::parse(text).ok_or_else(|| {
        format!("'{text}' is not a dialog answer: use accept, accept=TEXT or dismiss")
    })
}
