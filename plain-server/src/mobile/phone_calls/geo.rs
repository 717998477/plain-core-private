use super::resolvers::PhoneGeo;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocaleFacts {
    pub region: String,
    pub locale: String,
    #[serde(default)]
    pub network_region: String,
    #[serde(default)]
    pub sim_region: String,
}

pub(crate) type Runtime = plain_rs::phone::Service;

pub(crate) async fn lookup_many(
    runtime: Arc<Runtime>,
    numbers: Vec<String>,
    locale: LocaleFacts,
) -> Result<Vec<Option<PhoneGeo>>, tokio::task::JoinError> {
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    tokio::task::spawn_blocking(move || {
        let region = plain_rs::phone::default_region(
            &locale.network_region,
            &locale.sim_region,
            &locale.region,
        );
        numbers
            .iter()
            .map(|number| {
                runtime
                    .lookup(number, region, &locale.locale)
                    .map(|metadata| PhoneGeo {
                        country: metadata.country,
                        number_type: metadata.number_type,
                        carrier: metadata.carrier,
                        description: metadata.description,
                    })
            })
            .collect()
    })
    .await
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile/phone_calls/geo.rs"]
mod tests;
