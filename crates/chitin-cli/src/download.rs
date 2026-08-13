//! RCSB download execution and terminal progress reporting.

use std::{
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use chitin_databases::{
  Client, ClientConfig,
  providers::rcsb::{PdbId, StructureFormat},
};
use indicatif::{ProgressBar, ProgressStyle};

use crate::{error::CliError, output::resolve_output_path};

/// Downloads an RCSB artifact and writes it to the resolved output path.
///
/// # Parameters
///
/// * `raw_id` is the user-provided four-character PDB identifier.
/// * `format` selects PDB or mmCIF content.
/// * `output` is an optional file or directory override.
///
/// # Returns
///
/// `Ok(())` after the artifact has been written and reported to the terminal.
pub(crate) async fn download_rcsb(
  raw_id: String,
  format: StructureFormat,
  output: Option<PathBuf>,
) -> Result<(), CliError> {
  let id = PdbId::new(&raw_id)?;
  let destination = resolve_output_path(output, &id, format)?;
  let progress = ProgressBar::new_spinner();
  progress.enable_steady_tick(Duration::from_millis(100));
  if let Ok(style) = ProgressStyle::with_template("  {spinner} Downloading {bytes} ({elapsed})") {
    progress.set_style(style);
  }
  let callback_bar = progress.clone();
  let determinate = Arc::new(AtomicBool::new(false));
  let callback_determinate = determinate.clone();
  let client = Client::new(ClientConfig::default()).map_err(|error| {
    CliError::Rcsb(chitin_databases::providers::rcsb::RcsbDownloadError::Provider(
      chitin_databases::providers::rcsb::RcsbError::Transport(error),
    ))
  })?;
  client
    .rcsb()
    .download_structure_to_path(id.clone(), format, &destination, move |received, total| {
      if let Some(total) = total
        && total > 0
        && callback_determinate
          .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
          .is_ok()
      {
        if let Ok(style) =
          ProgressStyle::with_template("  Downloading [{bar:32.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        {
          callback_bar.set_style(style.progress_chars("##-"));
        }
        callback_bar.set_length(total);
      }
      callback_bar.set_position(received);
    })
    .await
    .map_err(crate::error::CliError::Rcsb)?;
  progress.finish_and_clear();
  println!("  ✓ Downloaded {id} ({})", format.label());
  println!("  ✓ Saved to {}", destination.display());
  Ok(())
}
