use tabref_page::{Block, Control, Item, cut_chars, squash};

use super::block_renderer::{BlockRenderer, title_of};
use super::control_text::control_text;

const DIALOG_TEXT: usize = 200;
const DIALOG_CONTROLS: usize = 10;
const FIELD_ROLES: &[&str] = &["textfield", "combobox", "listbox"];

impl BlockRenderer<'_> {
    /// A dialog or overlay lifted to the top of the view. One that only
    /// asks for a choice (a consent banner, a confirmation) prints as its
    /// title with the start of its text, then one line of its controls; one
    /// with fields to fill in, or many controls, prints whole.
    pub(crate) fn render_dialog(&mut self, dialog: &Block) {
        let mut text = String::new();
        let mut controls: Vec<&Control> = Vec::new();
        gather(dialog, &mut text, &mut controls);
        let has_field = controls.iter().any(|control| FIELD_ROLES.contains(&control.role.as_str()));
        if has_field || controls.len() > DIALOG_CONTROLS {
            self.render_block(dialog, 0);
            return;
        }
        let title = title_of(dialog);
        let text = cut_chars(&squash(&text), DIALOG_TEXT);
        self.push(0, format!("{title}: {text}").trim_end().to_owned(), None);
        let tokens: Vec<String> = controls
            .iter()
            .map(|control| control_text(control, self.options, true))
            .collect();
        self.push(1, tokens.join(" "), None);
    }
}

fn gather<'a>(block: &'a Block, text: &mut String, controls: &mut Vec<&'a Control>) {
    for item in &block.items {
        match item {
            Item::Text(run) => text.push_str(run),
            Item::Break => text.push(' '),
            Item::Control(control) => controls.push(control),
            Item::Block(inner) => {
                text.push(' ');
                gather(inner, text, controls);
            }
        }
    }
}
