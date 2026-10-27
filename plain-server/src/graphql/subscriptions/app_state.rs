use super::{events, models::*};
use crate::graphql::resolvers::casting::sender::DlnaSenderState;
use crate::services::settings::update_download::AppUpdateDownload;
use async_graphql::{Context, Result, Subscription};
use futures_util::stream::BoxStream;
#[derive(Default)]
pub struct AppStateSubscriptions;
#[Subscription]
impl AppStateSubscriptions {
    async fn network_changed(&self, ctx: &Context<'_>) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "NETWORK_CHANGED").await
    }
    async fn http_server_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "HTTP_SERVER_STATE_CHANGED").await
    }
    async fn audio_library_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "AUDIO_LIBRARY_UPDATED").await
    }
    async fn clipboard_history_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "H_CLIPBOARD_HISTORY_UPDATED").await
    }
    async fn peers_changed(&self, ctx: &Context<'_>) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "PEER_UPDATED").await
    }
    async fn channels_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<ChannelChange>>> {
        events::stream(ctx, "CHANNELS_UPDATED").await
    }
    async fn mdns_changed(&self, ctx: &Context<'_>) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "MDNS_UPDATED").await
    }
    async fn nearby_devices_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "NEARBY_DEVICES_UPDATED").await
    }
    async fn downloads_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "DOWNLOAD_PROGRESS").await
    }
    async fn shared_folder_downloads_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "SHARED_FOLDER_DOWNLOAD_UPDATED").await
    }
    async fn dlna_renderer_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "DLNA_RENDERER_UPDATED").await
    }
    async fn dlna_sender_updated(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<DlnaSenderState>>> {
        events::stream(ctx, "DLNA_SENDER_UPDATED").await
    }
    async fn peer_transports_updated(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<Vec<PeerTransportState>>>> {
        events::stream(ctx, "PEER_TRANSPORT_UPDATED").await
    }
    async fn online_clients_updated(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<Vec<String>>>> {
        events::stream(ctx, "ONLINE_CLIENTS_UPDATED").await
    }
    async fn image_models_updated(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<ImageModelsUpdate>>> {
        events::stream(ctx, "IMAGE_MODELS_UPDATED").await
    }
    async fn app_update_download_updated(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<AppUpdateDownload>>> {
        events::stream(ctx, "APP_UPDATE_DOWNLOAD_UPDATED").await
    }
    async fn feed_sync_updated(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<FeedSyncUpdate>>> {
        events::stream(ctx, "FEED_SYNC_UPDATED").await
    }
    async fn audio_playback_changed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<AudioPlaybackChange>>> {
        events::stream(ctx, "AUDIO_PLAYBACK_UPDATED").await
    }
    async fn pairing_changed(&self, ctx: &Context<'_>) -> Result<BoxStream<'static, Result<bool>>> {
        events::stream(ctx, "PAIRING_STATE_CHANGED").await
    }
    async fn pairing_requested(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<PairingRequestNotice>>> {
        events::stream(ctx, "PAIRING_REQUEST_RECEIVED").await
    }
    async fn pairing_succeeded(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<PairingSuccessNotice>>> {
        events::stream(ctx, "PAIRING_SUCCESS").await
    }
    async fn pomodoro_action(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BoxStream<'static, Result<PomodoroAction>>> {
        events::stream(ctx, "POMODORO_ACTION").await
    }
}
