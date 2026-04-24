pub trait ConversionReporter: Send + Sync {
    fn emit_output_line(&self, line: &str);
}

pub(crate) struct NoopReporter;

impl ConversionReporter for NoopReporter {
    fn emit_output_line(&self, _line: &str) {}
}

pub(crate) fn noop_reporter() -> &'static NoopReporter {
    static REPORTER: NoopReporter = NoopReporter;
    &REPORTER
}
