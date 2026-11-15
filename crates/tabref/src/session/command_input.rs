use std::io::Read;

use tabref_core::AppError;

use super::Session;

impl Session {
    /// The command's standard input: what the client sent with the request
    /// when the command runs in the daemon, else this process's own stdin.
    /// A command that reads its input (such as a batch of steps) reads it
    /// here, so it works the same in both.
    pub(crate) fn command_stdin(&mut self) -> Result<String, AppError> {
        if let Some(input) = self.scope.stdin.take() {
            return Ok(input);
        }
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        Ok(input)
    }
}
