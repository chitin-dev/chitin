//! RCSB download execution and terminal progress reporting.

use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use chitin_databases::{
  Client, ClientConfig,
  providers::rcsb::{RcsbBatchDownloadEvent, RcsbBatchDownloadRequest, RcsbDownloadError, RcsbError},
};
use indicatif::{ProgressBar, ProgressStyle};

use chitin_command::RcsbDownloadArguments;

use crate::{error::CliError, output::resolve_output_path};

/// Downloads an RCSB artifact and writes it to the resolved output path.
///
/// # Parameters
///
/// * `arguments` contains validated identifiers, format, and output override.
///
/// # Returns
///
/// `Ok(())` after the artifact has been written and reported to the terminal.
pub(crate) async fn download_rcsb(arguments: RcsbDownloadArguments) -> Result<(), CliError> {
  let RcsbDownloadArguments { ids, format, output } = arguments;
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
