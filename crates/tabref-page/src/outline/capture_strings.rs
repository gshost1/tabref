use serde_json::Value;

/// The shared string table of a DOM capture.
pub(crate) struct Strings<'a>(pub(crate) &'a [&'a str]);

impl Strings<'_> {
    pub(crate) fn get(&self, index: &Value) -> Option<&str> {
        let index = usize::try_from(index.as_i64()?).ok()?;
        self.0.get(index).copied()
    }

    pub(crate) fn text(&self, index: Option<&Value>) -> String {
        index
            .and_then(|index| self.get(index))
            .unwrap_or_default()
            .to_owned()
    }
}
