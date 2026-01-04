use anyhow::Error;

/// Collects errors during processing to report at the end
pub struct ErrorCollector {
    errors: Vec<(String, Error)>,
}

impl ErrorCollector {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add an error with context
    pub fn add(&mut self, context: &str, error: Error) {
        self.errors.push((context.to_string(), error));
    }

    /// Check if any errors were collected
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Report all collected errors to stderr
    pub fn report(&self) {
        if self.errors.is_empty() {
            return;
        }

        eprintln!();
        eprintln!("Encountered {} error(s):", self.errors.len());
        for (context, error) in &self.errors {
            eprintln!("  - {}: {:#}", context, error);
        }
    }
}

impl Default for ErrorCollector {
    fn default() -> Self {
        Self::new()
    }
}
