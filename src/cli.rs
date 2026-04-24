use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "nlbn")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Fast EasyEDA/LCSC to KiCad converter with parallel downloads", long_about = None)]
pub struct Cli {
    /// LCSC component ID (e.g., C2040)
    #[arg(long, value_name = "ID", conflicts_with = "batch")]
    pub lcsc_id: Option<String>,

    /// Batch mode: read LCSC IDs from a file (one ID per line)
    #[arg(long, value_name = "FILE", conflicts_with = "lcsc_id")]
    pub batch: Option<PathBuf>,

    /// Convert symbol only
    #[arg(long)]
    pub symbol: bool,

    /// Convert footprint only
    #[arg(long)]
    pub footprint: bool,

    /// Convert 3D model only
    #[arg(long = "3d")]
    pub model_3d: bool,

    /// Convert all (symbol + footprint + 3D model)
    #[arg(long)]
    pub full: bool,

    /// Output directory path
    #[arg(short, long, default_value = ".")]
    pub output: PathBuf,

    /// Overwrite existing components
    #[arg(long)]
    pub overwrite: bool,

    /// Overwrite symbol output only
    #[arg(long)]
    pub overwrite_symbol: bool,

    /// Overwrite footprint output only
    #[arg(long)]
    pub overwrite_footprint: bool,

    /// Overwrite 3D model output only
    #[arg(long = "overwrite-3d")]
    pub overwrite_model_3d: bool,

    /// Use project-relative paths (KIPRJMOD) instead of footprint-relative paths for 3D models
    #[arg(long)]
    pub project_relative: bool,

    /// Override filled symbol rectangle color with #RRGGBB or #RRGGBBAA
    #[arg(long, value_name = "HEX")]
    pub symbol_fill_color: Option<String>,

    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,

    /// Continue on error in batch mode (skip failed components)
    #[arg(long)]
    pub continue_on_error: bool,

    /// Number of parallel downloads in batch mode (default: 4)
    #[arg(long, default_value = "4")]
    pub parallel: usize,
}
