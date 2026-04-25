use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use nlbn::{AppError, ConversionReporter, RunReporter, RunSummary};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct ConsoleReporter {
    is_batch: AtomicBool,
    progress: ProgressBar,
}

impl ConsoleReporter {
    pub fn new() -> Self {
        let progress = ProgressBar::new(0);
        progress.set_draw_target(ProgressDrawTarget::hidden());
        progress
            .set_style(
                ProgressStyle::default_bar()
                    .template(
                        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {per_sec} ETA: {eta}",
                    )
                    .unwrap()
                    .progress_chars("█▉▊▋▌▍▎▏  "),
            );

        Self {
            is_batch: AtomicBool::new(false),
            progress,
        }
    }

    pub fn report_no_work(&self) {
        println!("All components already completed.");
    }

    pub fn report_summary(&self, summary: &RunSummary) {
        if summary.is_batch {
            println!("\n{}", "=".repeat(60));
            println!("Batch conversion complete!");
            println!(
                "Total: {} | Success: {} | Failed: {}",
                summary.total, summary.success, summary.failed
            );

            if !summary.failed_ids.is_empty() {
                println!("\nFailed components:");
                for id in &summary.failed_ids {
                    println!("  - {}", id);
                }
            }

            println!("Output directory: {}", summary.output_dir.display());
            println!("{}", "=".repeat(60));
        } else {
            println!("\n✓ Conversion complete!");
            println!("Output directory: {}", summary.output_dir.display());
        }
    }
}

impl RunReporter for ConsoleReporter {
    fn on_resume_skipped(&self, skipped: usize) {
        log::info!(
            "Resuming: skipping {} already completed components",
            skipped
        );
    }

    fn on_batch_started(&self, is_batch: bool, total_count: usize, parallel: usize) {
        self.is_batch.store(is_batch, Ordering::Relaxed);
        self.progress.set_length(total_count as u64);
        if is_batch {
            self.progress.set_draw_target(ProgressDrawTarget::stderr());
        }

        if !is_batch {
            return;
        }
        log::info!("Batch mode: processing {} components", total_count);
        if parallel > 1 {
            log::info!("Parallel downloads: {} threads", parallel);
        }
    }

    fn on_component_started(&self, lcsc_id: &str) {
        if self.is_batch.load(Ordering::Relaxed) {
            self.progress.set_message(lcsc_id.to_string());
        } else {
            log::info!("Starting conversion for LCSC ID: {}", lcsc_id);
        }
    }

    fn on_component_succeeded(&self, lcsc_id: &str) {
        if self.is_batch.load(Ordering::Relaxed) {
            self.progress.println(format!("✓ {}", lcsc_id));
            self.progress.inc(1);
        }
    }

    fn on_component_failed(&self, lcsc_id: &str, error: &AppError, continued: bool) {
        if self.is_batch.load(Ordering::Relaxed) {
            self.progress.println(format!("✗ {} - {}", lcsc_id, error));
            self.progress.inc(1);
        } else if continued {
            eprintln!("✗ Failed: {} - {}", lcsc_id, error);
        }

        log::error!("Failed to process {}: {}", lcsc_id, error);
    }

    fn on_task_panicked(&self, error: &str) {
        log::error!("Task panicked: {}", error);
    }

    fn finish(&self) {
        if self.is_batch.load(Ordering::Relaxed) {
            self.progress.finish_and_clear();
        }
    }
}

impl ConversionReporter for ConsoleReporter {
    fn emit_output_line(&self, line: &str) {
        if self.is_batch.load(Ordering::Relaxed) {
            self.progress.println(line);
        } else {
            println!("{}", line);
        }
    }
}
