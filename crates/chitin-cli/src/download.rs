//! RCSB download execution and terminal progress reporting.

use std::{
  path::PathBuf,
  sync::{Arc, Mutex},
  time::Duration,
};

use chitin_databases::{
  Client, ClientConfig,
  providers::rcsb::{
    PdbId, RcsbBatchDownloadEvent, RcsbBatchDownloadRequest, RcsbDownloadError, RcsbError, StructureFormat,
  },
};
use indicatif::{ProgressBar, ProgressStyle};

use crate::{error::CliError, output::resolve_output_path};

/// Downloads an RCSB artifact and writes it to the resolved output path.
///
/// # Parameters
///
/// * `raw_id` is the user-provided comma-separated list of four-character PDB
///   identifiers such as 4HHB,1YTH.
/// * `format` selects PDB or mmCIF content.
/// * `output` is an optional file or directory override.
///
/// # Returns
///
/// `Ok(())` after the artifact has been written and reported to the terminal.
pub(crate) async fn download_rcsb(
  raw_ids: String,
  format: StructureFormat,
  output: Option<PathBuf>,
) -> Result<(), CliError> {
  // we use `,` as the splitter of different PDB IDs.
  let ids = PdbId::parse_many(&raw_ids)?;
  let multiple = ids.len() > 1;
  let client = Client::new(ClientConfig::default())
    .map_err(|error| CliError::Rcsb(RcsbDownloadError::Provider(RcsbError::Transport(error))))?;
  let requests = ids
    .into_iter()
    .map(|id| {
      let destination = resolve_output_path(output.clone(), &id, format, multiple)?;
      Ok(RcsbBatchDownloadRequest {
        id,
        format,
        destination,
      })
    })
    .collect::<Result<Vec<_>, CliError>>()?;
  let progress: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
  let callback_progress = progress.clone();
  client
    .rcsb()
    .download_structures_to_paths(&requests, move |event| match event {
      RcsbBatchDownloadEvent::Started { index, total, id } => {
        let bar = ProgressBar::new_spinner();
        bar.enable_steady_tick(Duration::from_millis(100));
        if let Ok(style) = ProgressStyle::with_template("  {spinner} Downloading {bytes} ({elapsed})") {
          bar.set_style(style);
        }
        eprintln!("  Downloading {index}/{total}: {id}");
        if let Ok(mut current) = callback_progress.lock() {
          *current = Some(bar);
        }
      }
      RcsbBatchDownloadEvent::Progress {
        received, total_bytes, ..
      } => {
        if let Ok(current) = callback_progress.lock()
          && let Some(bar) = current.as_ref()
        {
          if let Some(total_bytes) = total_bytes {
            if bar.length().is_none() {
              if let Ok(style) =
                ProgressStyle::with_template("  Downloading [{bar:32.cyan/blue}] {bytes}/{total_bytes} ({eta})")
              {
                bar.set_style(style.progress_chars("##-"));
              }
              bar.set_length(total_bytes);
            }
          }
          bar.set_position(received);
        }
      }
      RcsbBatchDownloadEvent::Completed { id, path, .. } => {
        if let Ok(mut current) = callback_progress.lock()
          && let Some(bar) = current.take()
        {
          bar.finish_and_clear();
        }
        println!("  ✓ Downloaded {id} ({})", format.label());
        println!("  ✓ Saved to {}", path.display());
      }
    })
    .await
    .map_err(CliError::Rcsb)
}
