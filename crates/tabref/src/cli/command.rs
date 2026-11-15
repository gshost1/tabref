use std::path::PathBuf;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Navigate the current tab and print a snapshot of the loaded page
    Open {
        url: String,
        /// Start Chrome with a throwaway profile when no browser is reachable
        #[arg(long)]
        launch: bool,
        /// With --launch: show the browser window instead of running headless
        #[arg(long)]
        headed: bool,
    },
    /// Print the page view: text in reading order, controls with refs
    Snap {
        /// Print navigation, header and footer whole, with a larger budget
        #[arg(short = 'a', long)]
        all: bool,
        /// Print only the part of the page inside this ref
        #[arg(long, value_name = "REF")]
        root: Option<String>,
        /// Rows of long tables and lists to print: 21-40, 21- or 5
        #[arg(long, value_name = "FROM-TO")]
        rows: Option<String>,
        /// Print every line whole: no paragraph, code, dialog or option cuts
        #[arg(long)]
        full: bool,
        /// Start at this line of the view (the cut markers name it)
        #[arg(long, value_name = "LINE", default_value_t = 1)]
        from: usize,
    },
    /// Click an element
    Click {
        #[arg(value_name = "REF")]
        target: String,
    },
    /// Replace a field's text and read it back
    Fill {
        #[arg(value_name = "REF")]
        target: String,
        text: String,
    },
    /// Choose an option of a <select> by its label, value or text
    Select {
        #[arg(value_name = "REF")]
        target: String,
        option: String,
    },
    /// Tick a checkbox, radio button or switch
    Check {
        #[arg(value_name = "REF")]
        target: String,
    },
    /// Clear a checkbox or switch
    Uncheck {
        #[arg(value_name = "REF")]
        target: String,
    },
    /// Press a key (Enter, Tab, Escape, ArrowDown, Control+a, ...), in REF when given
    Press {
        key: String,
        #[arg(value_name = "REF")]
        target: Option<String>,
    },
    /// Move the pointer over an element
    Hover {
        #[arg(value_name = "REF")]
        target: String,
    },
    /// Scroll the page (down, up, left, right) or bring REF into view
    Scroll {
        /// down, up, left, right, or a ref to scroll into view
        #[arg(default_value = "down", value_name = "DIRECTION|REF")]
        direction: String,
        /// Scroll the container that holds this ref instead of the page
        #[arg(value_name = "REF")]
        target: Option<String>,
        /// Distance in CSS pixels (default: most of a screen)
        #[arg(long)]
        px: Option<u32>,
    },
    /// Set the files of an <input type=file>
    Upload {
        #[arg(value_name = "REF")]
        target: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Search the whole page, text included, for TEXT
    Find {
        text: String,
        /// Only nodes of this role
        #[arg(long)]
        role: Option<String>,
    },
    /// Print the main content as text, a section, or one element
    Read {
        #[arg(value_name = "REF")]
        target: Option<String>,
        /// The main content (the default when no REF is given)
        #[arg(long, conflicts_with_all = ["target", "section"])]
        main: bool,
        /// The section under the heading with this text (exact match first)
        #[arg(long, value_name = "HEADING", conflicts_with = "target")]
        section: Option<String>,
        /// Rows of long tables and lists to print: 21-40, 21- or 5
        #[arg(long, value_name = "FROM-TO")]
        rows: Option<String>,
        /// Start at this line of the text (the cut markers name it)
        #[arg(long, value_name = "LINE", default_value_t = 1)]
        from: usize,
    },
    /// Wait until a condition holds (bounded by --timeout)
    Wait {
        /// Until the page shows this text
        #[arg(long)]
        text: Option<String>,
        /// Until the page no longer shows this text
        #[arg(long)]
        gone: Option<String>,
        /// Until the address contains this
        #[arg(long)]
        url: Option<String>,
        /// Until the network has been quiet for 500 ms
        #[arg(long)]
        idle: bool,
    },
    /// Evaluate a JavaScript expression in the page and print its value
    Js { expression: String },
    /// List the open tabs
    Tabs,
    /// Switch to a tab by index or id
    Tab { tab: String },
    /// Close a tab (the current one by default)
    Close { tab: Option<String> },
    /// Print the agent guide
    Skill,
}

impl Command {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Open { .. } => "open",
            Self::Snap { .. } => "snap",
            Self::Click { .. } => "click",
            Self::Fill { .. } => "fill",
            Self::Select { .. } => "select",
            Self::Check { .. } => "check",
            Self::Uncheck { .. } => "uncheck",
            Self::Press { .. } => "press",
            Self::Hover { .. } => "hover",
            Self::Scroll { .. } => "scroll",
            Self::Upload { .. } => "upload",
            Self::Find { .. } => "find",
            Self::Read { .. } => "read",
            Self::Wait { .. } => "wait",
            Self::Js { .. } => "js",
            Self::Tabs => "tabs",
            Self::Tab { .. } => "tab",
            Self::Close { .. } => "close",
            Self::Skill => "skill",
        }
    }
}
