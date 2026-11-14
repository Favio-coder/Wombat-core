use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use wombat_risk::{evaluate, load, CellRisk};

#[derive(Parser)]
#[command(
    name = "wombat-cli",
    about = "CLI de evidencia auditable para WOMBAT",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Valida evidencia de entrada y prepara el ranking
    Ingest {
        #[arg(long)]
        cells: PathBuf,

        #[arg(long)]
        manifest: PathBuf,

        #[arg(long)]
        config: PathBuf,
    },

    /// Exporta un bundle estático para el dashboard
    ExportStatic {
        #[arg(long)]
        input: PathBuf,

        #[arg(long)]
        out: PathBuf,
    },

    /// Verifica un bloque WONB firmado
    Verify {
        #[arg(long)]
        file: PathBuf,
    },
}

#[derive(Debug, Deserialize)]
struct PipelineManifest {
    aoi_id: String,
    comparable: bool,
}

fn read_manifest(path: &Path) -> Result<PipelineManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("no se pudo leer el manifest: {}", path.display()))?;

    serde_json::from_str(&content)
        .with_context(|| format!("manifest JSON inválido: {}", path.display()))
}

#[derive(Debug)]
struct IngestSummary {
    aoi_id: String,
    cells: Vec<CellRisk>,
    ranked_count: usize,
    excluded_quality_count: usize,
}

fn ingest(manifest_path: &Path, cells_path: &Path, config_path: &Path) -> Result<IngestSummary> {
    let manifest = read_manifest(manifest_path)?;
    ingest_manifest(manifest, cells_path, config_path)
}

fn ingest_manifest(
    manifest: PipelineManifest,
    cells_path: &Path,
    config_path: &Path,
) -> Result<IngestSummary> {
    if !manifest.comparable {
        bail!("ingest rechazado: las escenas no son comparables; no se puede calcular ranking");
    }

    let inputs = load::load_inputs(&manifest.aoi_id, cells_path)
        .with_context(|| format!("no se pudo cargar cells.geojson: {}", cells_path.display()))?;
    let config = load::load_config(config_path).with_context(|| {
        format!(
            "no se pudo cargar la configuración de riesgo: {}",
            config_path.display()
        )
    })?;
    let cells = evaluate(&config, &inputs).context("no se pudo calcular el ranking")?;
    let ranked_count = cells.iter().filter(|cell| cell.is_ranked()).count();
    let excluded_quality_count = cells.len() - ranked_count;

    Ok(IngestSummary {
        aoi_id: inputs.aoi_id,
        cells,
        ranked_count,
        excluded_quality_count,
    })
}

fn print_ingest_summary(summary: &IngestSummary) -> Result<()> {
    println!("AOI: {}", summary.aoi_id);
    println!("Celdas totales: {}", summary.cells.len());
    println!("Celdas rankeadas: {}", summary.ranked_count);
    println!(
        "Celdas excluidas por calidad: {}",
        summary.excluded_quality_count
    );
    println!("Top 3:");

    for cell in summary.cells.iter().filter(|cell| cell.is_ranked()).take(3) {
        let rank = cell.rank.context("celda rankeada sin rank")?;
        let score_bp = cell.score_bp.context("celda rankeada sin score")?;
        let status = serde_json::to_value(cell.status)?;
        let status = status
            .as_str()
            .context("estado de riesgo no serializó como texto")?;
        println!(
            "  rank={rank} cell_id={} score={score_bp} bp status={status}",
            cell.cell_id
        );
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ingest {
            cells,
            manifest,
            config,
        } => {
            let summary = ingest(&manifest, &cells, &config)?;
            print_ingest_summary(&summary)?;
        }
        Commands::ExportStatic { input, out } => {
            println!("Pendiente de exportar: input={input:?}, out={out:?}");
        }
        Commands::Verify { file } => {
            println!("Pendiente de verificar: file={file:?}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn manifest(comparable: bool) -> PipelineManifest {
        PipelineManifest {
            aoi_id: "piura-demo".to_owned(),
            comparable,
        }
    }

    #[test]
    fn ingest_ranks_the_canonical_piura_fixture() {
        let root = repo_root();
        let summary = ingest(
            &root.join("fixtures/piura-demo/manifest.json"),
            &root.join("contracts/v1/fixtures/bundle/piura-demo/v0/cells.geojson"),
            &root.join("config/risk-config.json"),
        )
        .unwrap();

        assert_eq!(summary.aoi_id, "piura-demo");
        assert_eq!(summary.cells.len(), 16);
        assert_eq!(summary.ranked_count, 15);
        assert_eq!(summary.excluded_quality_count, 1);
        assert_eq!(
            summary
                .cells
                .iter()
                .take(3)
                .map(|cell| (cell.rank, cell.cell_id.as_str(), cell.score_bp))
                .collect::<Vec<_>>(),
            [
                (Some(1), "piura-demo-r02c01", Some(7636)),
                (Some(2), "piura-demo-r01c01", Some(6593)),
                (Some(3), "piura-demo-r02c02", Some(6365)),
            ]
        );
    }

    #[test]
    fn ingest_rejects_non_comparable_manifest() {
        let root = repo_root();
        let error = ingest_manifest(
            manifest(false),
            Path::new("cells.geojson"),
            &root.join("config/risk-config.json"),
        )
        .unwrap_err();

        assert!(error.to_string().contains("no son comparables"));
    }

    #[test]
    fn ingest_rejects_missing_or_invalid_cells_input() {
        let root = repo_root();
        let config = root.join("config/risk-config.json");

        let missing_error = ingest_manifest(
            manifest(true),
            &root.join("fixtures/piura-demo/missing-cells.geojson"),
            &config,
        )
        .unwrap_err();
        assert!(missing_error.to_string().contains("missing-cells.geojson"));

        let invalid_error = ingest_manifest(
            manifest(true),
            &root.join("fixtures/piura-demo/manifest.json"),
            &config,
        )
        .unwrap_err();
        assert!(invalid_error.to_string().contains("cells.geojson"));
    }
}
