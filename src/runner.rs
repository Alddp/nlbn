use crate::checkpoint::CheckpointManager;
use crate::{
    AppError, Cli, ComponentConversionRequest, ConversionReporter, EasyedaApi, LibraryManager,
    Result, RunRequest, footprint_converter, model_converter, symbol_converter,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;

pub trait RunReporter: ConversionReporter + Send + Sync {
    fn on_resume_skipped(&self, skipped: usize);
    fn on_batch_started(&self, is_batch: bool, total_count: usize, parallel: usize);
    fn on_component_started(&self, lcsc_id: &str);
    fn on_component_succeeded(&self, lcsc_id: &str);
    fn on_component_failed(&self, lcsc_id: &str, error: &AppError, continued: bool);
    fn on_task_panicked(&self, error: &str);
    fn finish(&self);
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub failed_ids: Vec<String>,
    pub output_dir: PathBuf,
    pub is_batch: bool,
}

struct PreparedRun {
    request: Arc<RunRequest>,
    api: Arc<EasyedaApi>,
    lib_manager: Arc<LibraryManager>,
    checkpoint: Arc<CheckpointManager>,
    is_batch: bool,
}

impl PreparedRun {
    fn prepare(request: RunRequest, reporter: &dyn RunReporter) -> Result<Self> {
        let mut request = request;
        let lcsc_ids = std::mem::take(&mut request.lcsc_ids);
        let is_batch = lcsc_ids.len() > 1;

        let lib_manager = Arc::new(LibraryManager::with_overwrite(
            &request.run.output,
            request.run.overwrite,
        ));
        lib_manager.create_directories()?;

        let checkpoint = Arc::new(CheckpointManager::load(
            request.run.output.join(".checkpoint"),
        )?);
        let completed_ids = checkpoint.completed_ids();
        let before = lcsc_ids.len();
        request.lcsc_ids =
            filter_pending_lcsc_ids(lcsc_ids, &completed_ids, is_batch, request.run.overwrite);
        if is_batch && !request.run.overwrite && before != request.lcsc_ids.len() {
            reporter.on_resume_skipped(before - request.lcsc_ids.len());
        }

        Ok(Self {
            request: Arc::new(request),
            api: Arc::new(EasyedaApi::new()),
            lib_manager,
            checkpoint,
            is_batch,
        })
    }

    fn total_count(&self) -> usize {
        self.request.lcsc_ids.len()
    }
}

pub async fn run_with_reporter(
    args: Cli,
    reporter: Arc<dyn RunReporter>,
) -> Result<Option<RunSummary>> {
    let request = RunRequest::try_from(args)?;
    let prepared = PreparedRun::prepare(request, reporter.as_ref())?;
    if prepared.total_count() == 0 {
        return Ok(None);
    }

    reporter.on_batch_started(
        prepared.is_batch,
        prepared.total_count(),
        prepared.request.run.parallel,
    );
    let summary = execute(prepared, reporter).await?;
    Ok(Some(summary))
}

async fn execute(prepared: PreparedRun, reporter: Arc<dyn RunReporter>) -> Result<RunSummary> {
    let total_count = prepared.total_count();
    let success_count = Arc::new(AtomicUsize::new(0));
    let failed_count = Arc::new(AtomicUsize::new(0));
    let successful_ids = Arc::new(Mutex::new(Vec::new()));
    let failed_ids = Arc::new(Mutex::new(Vec::new()));

    let run_result = if prepared.is_batch && prepared.request.run.parallel > 1 {
        run_parallel(
            &prepared,
            Arc::clone(&reporter),
            Arc::clone(&success_count),
            Arc::clone(&failed_count),
            Arc::clone(&successful_ids),
            Arc::clone(&failed_ids),
        )
        .await
    } else {
        run_sequential(
            &prepared,
            Arc::clone(&reporter),
            Arc::clone(&success_count),
            Arc::clone(&failed_count),
            Arc::clone(&successful_ids),
            Arc::clone(&failed_ids),
        )
        .await
    };

    let finalize_result = finalize_run(&prepared, &successful_ids).await;
    reporter.finish();
    finalize_result?;
    run_result?;

    let failed_ids = failed_ids.lock().await.clone();
    Ok(RunSummary {
        total: total_count,
        success: success_count.load(Ordering::Relaxed),
        failed: failed_count.load(Ordering::Relaxed),
        failed_ids,
        output_dir: prepared.request.run.output.clone(),
        is_batch: prepared.is_batch,
    })
}

async fn run_parallel(
    prepared: &PreparedRun,
    reporter: Arc<dyn RunReporter>,
    success_count: Arc<AtomicUsize>,
    failed_count: Arc<AtomicUsize>,
    successful_ids: Arc<Mutex<Vec<String>>>,
    failed_ids: Arc<Mutex<Vec<String>>>,
) -> Result<()> {
    let semaphore = Arc::new(Semaphore::new(prepared.request.run.parallel));
    let mut join_set = JoinSet::new();
    let continue_on_error = prepared.request.run.continue_on_error;

    for lcsc_id in prepared.request.lcsc_ids.iter().cloned() {
        let semaphore = Arc::clone(&semaphore);
        let reporter = Arc::clone(&reporter);
        let request = Arc::clone(&prepared.request);
        let api = Arc::clone(&prepared.api);
        let lib_manager = Arc::clone(&prepared.lib_manager);
        let success_count = Arc::clone(&success_count);
        let failed_count = Arc::clone(&failed_count);
        let successful_ids = Arc::clone(&successful_ids);
        let failed_ids = Arc::clone(&failed_ids);

        join_set.spawn(async move {
            let _permit = semaphore.acquire().await.expect("semaphore closed");
            reporter.on_component_started(&lcsc_id);

            match process_component(
                &request.component,
                &api,
                &lib_manager,
                &lcsc_id,
                reporter.as_ref(),
            )
            .await
            {
                Ok(_) => {
                    success_count.fetch_add(1, Ordering::Relaxed);
                    successful_ids.lock().await.push(lcsc_id.clone());
                    reporter.on_component_succeeded(&lcsc_id);
                    Ok::<(), AppError>(())
                }
                Err(error) => {
                    failed_count.fetch_add(1, Ordering::Relaxed);
                    failed_ids.lock().await.push(lcsc_id.clone());
                    reporter.on_component_failed(&lcsc_id, &error, continue_on_error);
                    Err(error)
                }
            }
        });
    }

    let mut first_error = None;
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                if !continue_on_error && first_error.is_none() {
                    first_error = Some(error);
                    join_set.abort_all();
                }
            }
            Err(error) if error.is_cancelled() => {}
            Err(error) => {
                failed_count.fetch_add(1, Ordering::Relaxed);
                failed_ids.lock().await.push("<task panic>".to_string());
                reporter.on_task_panicked(&error.to_string());
                if !continue_on_error && first_error.is_none() {
                    first_error = Some(AppError::Other(format!("Task panicked: {}", error)));
                    join_set.abort_all();
                }
            }
        }
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    Ok(())
}

async fn run_sequential(
    prepared: &PreparedRun,
    reporter: Arc<dyn RunReporter>,
    success_count: Arc<AtomicUsize>,
    failed_count: Arc<AtomicUsize>,
    successful_ids: Arc<Mutex<Vec<String>>>,
    failed_ids: Arc<Mutex<Vec<String>>>,
) -> Result<()> {
    for lcsc_id in &prepared.request.lcsc_ids {
        reporter.on_component_started(lcsc_id);

        match process_component(
            &prepared.request.component,
            &prepared.api,
            &prepared.lib_manager,
            lcsc_id,
            reporter.as_ref(),
        )
        .await
        {
            Ok(_) => {
                success_count.fetch_add(1, Ordering::Relaxed);
                successful_ids.lock().await.push(lcsc_id.clone());
                reporter.on_component_succeeded(lcsc_id);
            }
            Err(error) => {
                failed_count.fetch_add(1, Ordering::Relaxed);
                failed_ids.lock().await.push(lcsc_id.clone());

                if prepared.request.run.continue_on_error {
                    reporter.on_component_failed(lcsc_id, &error, true);
                } else {
                    reporter.on_component_failed(lcsc_id, &error, false);
                    return Err(error);
                }
            }
        }
    }

    Ok(())
}

async fn finalize_run(
    prepared: &PreparedRun,
    successful_ids: &Arc<Mutex<Vec<String>>>,
) -> Result<()> {
    prepared.lib_manager.flush_symbol_libraries()?;
    let successful_ids = successful_ids.lock().await.clone();
    prepared.checkpoint.append_completed_ids(&successful_ids)?;
    Ok(())
}

fn filter_pending_lcsc_ids(
    lcsc_ids: Vec<String>,
    completed_ids: &HashSet<String>,
    is_batch: bool,
    overwrite: bool,
) -> Vec<String> {
    if !is_batch || overwrite || completed_ids.is_empty() {
        return lcsc_ids;
    }

    lcsc_ids
        .into_iter()
        .filter(|id| !completed_ids.contains(id))
        .collect()
}

async fn process_component(
    request: &ComponentConversionRequest,
    api: &EasyedaApi,
    lib_manager: &LibraryManager,
    lcsc_id: &str,
    reporter: &dyn RunReporter,
) -> Result<()> {
    let component_data = api.get_component_data(lcsc_id).await?;

    log::info!("Fetched component: {}", component_data.title);

    if request.convert_symbol {
        log::info!("Converting symbol...");
        symbol_converter::convert_symbol_with_options_and_reporter(
            &request.symbol,
            &component_data,
            lib_manager,
            lcsc_id,
            reporter,
        )?;
    }

    if request.convert_model_3d {
        model_converter::convert_3d_model_with_reporter(
            api,
            &component_data,
            lib_manager,
            lcsc_id,
            reporter,
        )
        .await?;
    }

    if request.convert_footprint {
        log::info!("Converting footprint...");
        footprint_converter::convert_footprint_with_options_and_reporter(
            &request.footprint,
            &component_data,
            lib_manager,
            lcsc_id,
            reporter,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::filter_pending_lcsc_ids;
    use std::collections::HashSet;

    #[test]
    fn batch_resume_skips_completed_ids_without_overwrite() {
        let ids = vec!["C1".to_string(), "C2".to_string(), "C3".to_string()];
        let completed = HashSet::from(["C2".to_string()]);

        let filtered = filter_pending_lcsc_ids(ids, &completed, true, false);

        assert_eq!(filtered, vec!["C1".to_string(), "C3".to_string()]);
    }

    #[test]
    fn batch_resume_keeps_completed_ids_when_overwrite_is_enabled() {
        let ids = vec!["C1".to_string(), "C2".to_string(), "C3".to_string()];
        let completed = HashSet::from(["C2".to_string()]);

        let filtered = filter_pending_lcsc_ids(ids.clone(), &completed, true, true);

        assert_eq!(filtered, ids);
    }
}
