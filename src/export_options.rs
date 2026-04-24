use crate::cli::Cli;
use crate::error::{AppError, EasyedaError, Result};
use crate::kicad::symbol_exporter::SymbolFillColor;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SymbolExportOptions {
    pub symbol_fill_color: Option<SymbolFillColor>,
    pub overwrite: bool,
}

impl SymbolExportOptions {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        Ok(Self {
            symbol_fill_color: parse_symbol_fill_color(cli.symbol_fill_color.as_deref())?,
            overwrite: cli.overwrite,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FootprintExportOptions {
    pub include_3d_model: bool,
    pub project_relative_3d: bool,
}

impl From<&Cli> for FootprintExportOptions {
    fn from(cli: &Cli) -> Self {
        Self {
            include_3d_model: cli.model_3d || cli.full,
            project_relative_3d: cli.project_relative,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComponentConversionRequest {
    pub convert_symbol: bool,
    pub convert_footprint: bool,
    pub convert_model_3d: bool,
    pub symbol: SymbolExportOptions,
    pub footprint: FootprintExportOptions,
}

impl ComponentConversionRequest {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        Ok(Self {
            convert_symbol: cli.symbol || cli.full,
            convert_footprint: cli.footprint || cli.full,
            convert_model_3d: cli.model_3d || cli.full,
            symbol: SymbolExportOptions::from_cli(cli)?,
            footprint: FootprintExportOptions::from(cli),
        })
    }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub output: PathBuf,
    pub overwrite: bool,
    pub continue_on_error: bool,
    pub parallel: usize,
}

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub lcsc_ids: Vec<String>,
    pub run: RunOptions,
    pub component: ComponentConversionRequest,
}

impl TryFrom<Cli> for RunRequest {
    type Error = crate::AppError;

    fn try_from(cli: Cli) -> Result<Self> {
        validate_cli(&cli)?;
        let lcsc_ids = resolve_lcsc_ids(&cli)?;
        let component = ComponentConversionRequest::from_cli(&cli)?;

        Ok(Self {
            lcsc_ids,
            run: RunOptions {
                output: cli.output,
                overwrite: cli.overwrite,
                continue_on_error: cli.continue_on_error,
                parallel: cli.parallel,
            },
            component,
        })
    }
}

fn validate_cli(cli: &Cli) -> Result<()> {
    if cli.lcsc_id.is_none() && cli.batch.is_none() {
        return Err(AppError::Other(
            "Either --lcsc-id or --batch must be specified".to_string(),
        ));
    }

    if let Some(id) = &cli.lcsc_id
        && (!id.starts_with('C') || id.len() < 2)
    {
        return Err(AppError::Easyeda(EasyedaError::InvalidLcscId(id.clone())));
    }

    if !cli.symbol && !cli.footprint && !cli.model_3d && !cli.full {
        return Err(AppError::Other(
            "At least one conversion option must be specified (--symbol, --footprint, --3d, or --full)"
                .to_string(),
        ));
    }

    let _ = parse_symbol_fill_color(cli.symbol_fill_color.as_deref())?;
    Ok(())
}

fn resolve_lcsc_ids(cli: &Cli) -> Result<Vec<String>> {
    if let Some(id) = &cli.lcsc_id {
        return Ok(vec![id.clone()]);
    }

    if let Some(batch_file) = &cli.batch {
        let content = std::fs::read_to_string(batch_file)
            .map_err(|error| AppError::io_context("read batch file", batch_file, error))?;

        let re = regex::Regex::new(r"C\d+").unwrap();
        let ids: Vec<String> = re
            .find_iter(&content)
            .map(|m| m.as_str().to_string())
            .collect();

        if ids.is_empty() {
            return Err(AppError::Other(
                "No valid LCSC IDs found in batch file".to_string(),
            ));
        }

        log::info!("Loaded {} LCSC IDs from batch file", ids.len());
        return Ok(ids);
    }

    Err(AppError::Other("No LCSC ID source specified".to_string()))
}

fn parse_symbol_fill_color(value: Option<&str>) -> Result<Option<SymbolFillColor>> {
    value.map(SymbolFillColor::parse).transpose()
}

#[cfg(test)]
mod tests {
    use super::RunRequest;
    use crate::Cli;
    use std::path::PathBuf;

    fn test_cli() -> Cli {
        Cli {
            lcsc_id: Some("C123456".to_string()),
            batch: None,
            symbol: false,
            footprint: false,
            model_3d: false,
            full: true,
            output: PathBuf::from("out"),
            overwrite: true,
            project_relative: true,
            symbol_fill_color: Some("#005C8FCC".to_string()),
            debug: false,
            continue_on_error: true,
            parallel: 8,
        }
    }

    #[test]
    fn run_request_expands_full_conversion_flags() {
        let request = RunRequest::try_from(test_cli()).unwrap();

        assert_eq!(request.lcsc_ids, vec!["C123456"]);
        assert!(request.component.convert_symbol);
        assert!(request.component.convert_footprint);
        assert!(request.component.convert_model_3d);
        assert!(request.component.footprint.include_3d_model);
        assert!(request.component.footprint.project_relative_3d);
        assert!(request.component.symbol.symbol_fill_color.is_some());
        assert_eq!(request.run.output, PathBuf::from("out"));
        assert!(request.run.overwrite);
        assert!(request.run.continue_on_error);
        assert_eq!(request.run.parallel, 8);
    }
}
