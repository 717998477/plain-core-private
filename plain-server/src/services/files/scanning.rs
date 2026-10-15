use crate::assembly::server::ServerState;
use anyhow::Result;

pub(crate) struct Progress {
    pub indexed: i32,
    pub total: Option<i32>,
    pub state: State,
    pub error: Option<String>,
}
pub(crate) enum State {
    Idle,
    Running,
    Paused,
    Stopped,
    Failed,
}
pub(crate) enum Command {
    Start(String),
    Rebuild(String),
    Pause,
    Resume,
    Stop,
    SetDirectories(Vec<String>),
}

async fn available(state: &ServerState) -> Result<()> {
    crate::services::capabilities::require(&state.host, "MEDIA_SCAN").await
}
pub(crate) async fn progress(state: &ServerState) -> Result<Progress> {
    available(state).await?;
    #[cfg(feature = "media")]
    {
        use super::media_scans::Status;
        let Some(snapshot) = state.media_scans.snapshot().map_err(anyhow::Error::msg)? else {
            return Ok(Progress {
                indexed: 0,
                total: Some(0),
                state: State::Idle,
                error: None,
            });
        };
        Ok(Progress {
            indexed: snapshot.processed_files.try_into()?,
            total: snapshot.total_files.map(i32::try_from).transpose()?,
            state: match snapshot.status {
                Status::Running => State::Running,
                Status::Paused => State::Paused,
                Status::Cancelling | Status::Cancelled => State::Stopped,
                Status::Completed => State::Idle,
                Status::Failed => State::Failed,
            },
            error: snapshot.error,
        })
    }
    #[cfg(not(feature = "media"))]
    anyhow::bail!("MEDIA_SCAN advertised without a scanner implementation")
}
pub(crate) async fn directories(state: &ServerState) -> Result<Vec<String>> {
    available(state).await?;
    Ok(plain_rs::prefs::media::source_directories(&state.prefs)?)
}
pub(crate) async fn execute(state: &ServerState, command: Command) -> Result<()> {
    available(state).await?;
    #[cfg(feature = "media")]
    {
        let catalog = crate::native::media_catalog::NativeMediaCatalog::new(state.clone());
        match command {
            Command::Start(root) | Command::Rebuild(root) => {
                catalog
                    .start_source_scan(&root)
                    .map_err(anyhow::Error::msg)?;
            }
            Command::Pause => {
                catalog.pause_file_scan().map_err(anyhow::Error::msg)?;
            }
            Command::Resume => {
                catalog.resume_file_scan().map_err(anyhow::Error::msg)?;
            }
            Command::Stop => {
                catalog.cancel_file_scan().map_err(anyhow::Error::msg)?;
            }
            Command::SetDirectories(dirs) => catalog
                .set_source_directories(&dirs)
                .map_err(anyhow::Error::msg)?,
        }
        Ok(())
    }
    #[cfg(not(feature = "media"))]
    {
        let _ = command;
        anyhow::bail!("MEDIA_SCAN advertised without a scanner implementation")
    }
}
