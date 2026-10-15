pub mod remote_v3;

use crate::auth::grandslam::GrandSlam;
use plist::Dictionary;
use plist_macro::plist;
use reqwest::header::HeaderMap;
use rootcause::prelude::*;
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::warn;
use web_time::SystemTime;

#[derive(Deserialize, Debug, Clone)]
pub struct AnisetteClientInfo {
    pub client_info: String,
    pub user_agent: String,
}

#[derive(Debug, Clone)]
pub struct AnisetteData {
    machine_id: String,
    one_time_password: String,
    pub routing_info: String,
    _device_description: String,
    device_unique_identifier: String,
    _local_user_id: String,
    generated_at: SystemTime,
}

// Some headers don't seem to be required. I guess not including them is technically more efficient soooo
impl AnisetteData {
    pub fn get_headers(&self) -> HashMap<String, String> {
        //let dt: DateTime<Utc> = Utc::now().round_subsecs(0);

        HashMap::from_iter([
            // (
            //     "X-Apple-I-Client-Time".to_string(),
            //     dt.format("%+").to_string().replace("+00:00", "Z"),
            // ),
            // ("X-Apple-I-SRL-NO".to_string(), serial),
            // ("X-Apple-I-TimeZone".to_string(), "UTC".to_string()),
            // ("X-Apple-Locale".to_string(), "en_US".to_string()),
            // ("X-Apple-I-MD-RINFO".to_string(), self.routing_info.clone()),
            // ("X-Apple-I-MD-LU".to_string(), self.local_user_id.clone()),
            (
                "X-Mme-Device-Id".to_string(),
                self.device_unique_identifier.clone(),
            ),
            ("X-Apple-I-MD".to_string(), self.one_time_password.clone()),
            ("X-Apple-I-MD-M".to_string(), self.machine_id.clone()),
            // (
            //     "X-Mme-Client-Info".to_string(),
            //     self.device_description.clone(),
            // ),
        ])
    }

    pub fn get_header_map(&self) -> Result<HeaderMap, Report> {
        let headers_map = self.get_headers();
        let mut header_map = HeaderMap::new();

        for (key, value) in headers_map {
            header_map.insert(
                reqwest::header::HeaderName::from_bytes(key.as_bytes())?,
                reqwest::header::HeaderValue::from_str(&value)?,
            );
        }

        Ok(header_map)
    }

    pub fn get_client_provided_data(&self) -> Dictionary {
        let headers = self.get_headers();

        let mut cpd = plist!(dict {
            "bootstrap": "true",
            "icscrec": "true",
            "loc": "en_US",
            "pbe": "false",
            "prkgen": "true",
            "svct": "iCloud"
        });

        for (key, value) in headers {
            cpd.insert(key.to_string(), plist::Value::String(value));
        }

        cpd
    }

    pub fn needs_refresh(&self) -> bool {
        let elapsed = self.generated_at.elapsed();
        match elapsed {
            Ok(elapsed) => elapsed.as_secs() > 60,
            Err(_) => {
                warn!("Unable to determine anisette data age, treating as expired");
                true
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum AnisetteDataResponse {
    Data(AnisetteData),
    NeedsProvisioning,
    IncorrectlyProvisioned,
}

#[cfg_attr(feature = "wasm", async_trait::async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait::async_trait)]
pub trait AnisetteProvider {
    async fn get_anisette_data(&self) -> Result<AnisetteDataResponse, Report>;

    async fn resync(&self, _sim: &str) -> Result<AnisetteDataResponse, Report> {
        Err(report!("anisette resync not supported by this provider"))
    }

    async fn get_client_info(&self) -> Result<AnisetteClientInfo, Report>;

    async fn provision(&mut self, gs: &Arc<GrandSlam>, force_recheck: bool) -> Result<(), Report>;
}

#[derive(Clone)]
pub struct AnisetteDataGenerator {
    provider: Arc<RwLock<dyn AnisetteProvider + Send + Sync>>,
    data: Option<Arc<AnisetteData>>,
}

impl AnisetteDataGenerator {
    pub fn new(provider: Arc<RwLock<dyn AnisetteProvider + Send + Sync>>) -> Self {
        AnisetteDataGenerator {
            provider,
            data: None,
        }
    }

    pub async fn get_anisette_data(
        &mut self,
        gs: Arc<GrandSlam>,
    ) -> Result<Arc<AnisetteData>, Report> {
        if let Some(data) = &self.data
            && !data.needs_refresh()
        {
            return Ok(data.clone());
        }

        // 3 attempts for the worst case: load anisette from storage, server says -45061 -> reprovision anisette -> get data
        let data = self.get_data_or_provision(&gs, 3).await?;
        let arc_data = Arc::new(data);
        self.data = Some(arc_data.clone());
        Ok(arc_data)
    }

    async fn get_data_or_provision(
        &mut self,
        gs: &Arc<GrandSlam>,
        max_iters: usize,
    ) -> Result<AnisetteData, Report> {
        if max_iters == 0 {
            bail!("Max anisette provisioning attempts reached");
        }
        // trying to avoid locking as write unless necessary for concurrency later on
        let provider = self.provider.read().await;
        match provider.get_anisette_data().await? {
            AnisetteDataResponse::Data(data) => Ok(data),
            status @ (AnisetteDataResponse::NeedsProvisioning
            | AnisetteDataResponse::IncorrectlyProvisioned) => {
                drop(provider);
                let mut provider_write = self.provider.write().await;
                provider_write
                    .provision(
                        gs,
                        matches!(status, AnisetteDataResponse::IncorrectlyProvisioned),
                    )
                    .await?;
                drop(provider_write);
                Box::pin(self.get_data_or_provision(gs, max_iters - 1)).await
            }
        }
    }

    pub async fn get_client_info(&self) -> Result<AnisetteClientInfo, Report> {
        let provider = self.provider.read().await;
        provider.get_client_info().await
    }

    /// Perform a login anisette resync with the server, returning corrected data.
    pub async fn resync(&mut self, sim: &str) -> Result<Arc<AnisetteData>, Report> {
        let provider = self.provider.read().await;
        let data = match provider.resync(sim).await? {
            AnisetteDataResponse::Data(data) => data,
            _ => {
                bail!("Anisette provider failed to provide data for resync, needs provisioning");
            }
        };
        let arc_data = Arc::new(data);
        self.data = Some(arc_data.clone());
        Ok(arc_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct ProvisioningFixture {
        stage: usize,
        always_invalid: bool,
        resets: Arc<Mutex<Vec<bool>>>,
    }

    #[async_trait::async_trait]
    impl AnisetteProvider for ProvisioningFixture {
        async fn get_anisette_data(&self) -> Result<AnisetteDataResponse, Report> {
            if self.always_invalid || self.stage == 1 {
                Ok(AnisetteDataResponse::IncorrectlyProvisioned)
            } else if self.stage == 0 {
                Ok(AnisetteDataResponse::NeedsProvisioning)
            } else {
                Ok(AnisetteDataResponse::Data(AnisetteData {
                    machine_id: "fixture-mdm".into(),
                    one_time_password: "fresh-otp".into(),
                    routing_info: "17106176".into(),
                    _device_description: "fixture".into(),
                    device_unique_identifier: "fixture-device".into(),
                    _local_user_id: "fixture-user".into(),
                    generated_at: SystemTime::now(),
                }))
            }
        }

        async fn get_client_info(&self) -> Result<AnisetteClientInfo, Report> {
            unreachable!("fixture never connects to Apple")
        }

        async fn provision(
            &mut self,
            _gs: &Arc<GrandSlam>,
            force_reset: bool,
        ) -> Result<(), Report> {
            self.resets.lock().unwrap().push(force_reset);
            self.stage += 1;
            Ok(())
        }
    }

    #[test]
    fn invalid_stored_provisioning_is_reset_and_fresh_headers_are_cached() {
        let resets = Arc::new(Mutex::new(Vec::new()));
        let mut generator =
            AnisetteDataGenerator::new(Arc::new(RwLock::new(ProvisioningFixture {
                stage: 0,
                always_invalid: false,
                resets: resets.clone(),
            })));
        let gs = Arc::new(GrandSlam::for_test("http://127.0.0.1/unused"));
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let data = generator.get_anisette_data(gs.clone()).await.unwrap();
            assert_eq!(data.get_headers()["X-Apple-I-MD"], "fresh-otp");
            let cached = generator.get_anisette_data(gs).await.unwrap();
            assert!(Arc::ptr_eq(&data, &cached));
        });
        assert_eq!(*resets.lock().unwrap(), vec![false, true]);
    }

    #[test]
    fn repeated_invalid_provisioning_stops_after_three_attempts() {
        let resets = Arc::new(Mutex::new(Vec::new()));
        let mut generator =
            AnisetteDataGenerator::new(Arc::new(RwLock::new(ProvisioningFixture {
                stage: 0,
                always_invalid: true,
                resets: resets.clone(),
            })));
        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(
                generator
                    .get_anisette_data(Arc::new(GrandSlam::for_test("http://127.0.0.1/unused"))),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Max anisette provisioning attempts reached")
        );
        assert_eq!(*resets.lock().unwrap(), vec![true; 3]);
    }
}
