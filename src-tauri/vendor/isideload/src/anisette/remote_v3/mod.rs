mod state;
mod websocket;

use std::sync::Arc;
use web_time::SystemTime;

use base64::prelude::*;
use plist_macro::plist;
use reqwest::{
    ClientBuilder,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue},
};
use reqwest_middleware::ClientBuilder as MwClientBuilder;
use rootcause::option_ext::OptionExt;
use rootcause::prelude::*;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::anisette::remote_v3::{state::AnisetteState, websocket::WsMessage};
use crate::anisette::{AnisetteClientInfo, AnisetteData, AnisetteProvider};
use crate::auth::grandslam::GrandSlam;
use crate::util::plist::PlistDataExtract;
use crate::util::storage::{SideloadingStorage, new_storage};
use crate::{SideloadError, anisette::remote_v3::websocket::AppWebSocket};

pub const DEFAULT_ANISETTE_V3_URL: &str = "https://ani.stikstore.app";

pub struct RemoteV3AnisetteProvider {
    pub state: Option<AnisetteState>,
    url: String,
    storage: Box<dyn SideloadingStorage>,
    serial_number: String,
    client: reqwest_middleware::ClientWithMiddleware,
    websocket_proxy: Option<String>,
}

impl RemoteV3AnisetteProvider {
    /// Create a new RemoteV3AnisetteProvider with the given URL and config path
    ///
    /// # Arguments
    /// - `url`: The URL of the remote anisette service
    /// - `storage`: The storage backend for anisette data
    /// - `serial_number`: The serial number of the device
    pub fn new(
        url: &str,
        storage: Box<dyn SideloadingStorage>,
        serial_number: String,
    ) -> Result<Self, Report> {
        Ok(Self {
            state: None,
            url: url.trim().trim_end_matches('/').to_string(),
            storage,
            serial_number,
            client: Self::build_reqwest_client(None)?,
            websocket_proxy: None,
        })
    }

    pub fn set_websocket_proxy(mut self, websocket_proxy: Option<String>) -> Result<Self, Report> {
        self.websocket_proxy = websocket_proxy;
        self.client = Self::build_reqwest_client(self.websocket_proxy.clone())?;
        Ok(self)
    }

    fn build_reqwest_client(
        websocket_proxy: Option<String>,
    ) -> Result<reqwest_middleware::ClientWithMiddleware, Report> {
        let builder = ClientBuilder::new();
        #[cfg(not(feature = "wasm"))]
        let builder = builder
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10));
        let client = builder.build()?;
        if let Some(websocket_proxy) = websocket_proxy {
            use crate::auth::middleware::WasmProxyMiddleware;

            Ok(MwClientBuilder::new(client)
                .with(WasmProxyMiddleware::new(websocket_proxy))
                .build())
        } else {
            Ok(MwClientBuilder::new(client).build())
        }
    }

    pub fn default() -> Result<Self, Report> {
        Self::new(
            DEFAULT_ANISETTE_V3_URL,
            Box::new(new_storage()),
            "0".to_string(),
        )
    }

    pub fn set_url(mut self, url: &str) -> RemoteV3AnisetteProvider {
        self.url = url.trim().trim_end_matches('/').to_string();
        self
    }

    pub fn set_storage(mut self, storage: Box<dyn SideloadingStorage>) -> RemoteV3AnisetteProvider {
        self.storage = storage;
        self
    }

    pub fn set_serial_number(mut self, serial_number: String) -> RemoteV3AnisetteProvider {
        self.serial_number = serial_number;
        self
    }

    async fn request_headers(&self, body: String) -> Result<reqwest::Response, Report> {
        let request = || {
            self.client
                .post(format!("{}/v3/get_headers", self.url))
                .header(CONTENT_TYPE, "application/json")
                .body(body.clone())
        };
        let response = request().send().await;
        #[cfg(not(feature = "wasm"))]
        let response = match response {
            Err(error) if error.is_connect() || error.is_timeout() || error.is_request() => {
                // This endpoint only generates headers for an existing identity;
                // retry the same v3 request once, without changing provisioning state.
                info!("Anisette transport failed; retrying v3 headers once");
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                request().send().await
            }
            response => response,
        };
        response
            .map_err(|error| match error {
                // Unwrap middleware for the existing reqwest error formatter. Custom
                // server URLs may contain credentials/tokens and must be stripped.
                reqwest_middleware::Error::Reqwest(error) => {
                    report!(error.without_url()).into_dynamic()
                }
                reqwest_middleware::Error::Middleware(_) => {
                    report!("Anisette request middleware failed")
                }
            })
            .context("Failed to get anisette headers (v3 POST)")
            .map_err(|report| report.into_dynamic())
    }
}

#[cfg_attr(feature = "wasm", async_trait::async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait::async_trait)]
impl AnisetteProvider for RemoteV3AnisetteProvider {
    async fn get_anisette_data(&self) -> Result<AnisetteData, Report> {
        let state = self
            .state
            .as_ref()
            .ok_or(SideloadError::AnisetteNotProvisioned)?;
        let adi_pb = state
            .adi_pb
            .as_ref()
            .ok_or(SideloadError::AnisetteNotProvisioned)?;
        let client_info = self.get_client_info().await?;

        let headers = self
            .request_headers(
                serde_json::json!({
                "identifier": BASE64_STANDARD.encode(state.keychain_identifier),
                "adi_pb": BASE64_STANDARD.encode(adi_pb)
                })
                .to_string(),
            )
            .await?
            .error_for_status()?
            .json::<AnisetteHeaders>()
            .await?;

        match headers {
            AnisetteHeaders::Headers {
                machine_id,
                one_time_password,
                routing_info,
            } => {
                let data = AnisetteData {
                    machine_id,
                    one_time_password,
                    routing_info,
                    _device_description: client_info.client_info.clone(),
                    device_unique_identifier: state.get_device_id(),
                    _local_user_id: hex::encode(state.get_md_lu()),
                    generated_at: SystemTime::now(),
                };

                Ok(data)
            }
            AnisetteHeaders::GetHeadersError { message } => {
                Err(report!("Failed to get anisette headers").attach(message))
            }
        }
    }

    async fn get_client_info(&self) -> Result<AnisetteClientInfo, Report> {
        Ok(AnisetteClientInfo {
            client_info:
                "<Mac15,7> <macOS;27.0;26A5378j> <com.apple.AuthKit/1 (com.apple.akd/1.0)>"
                    .to_string(),
            user_agent: "akd/1.0 CFNetwork/808.1.4".to_string(),
        })
    }

    fn needs_provisioning(&self) -> Result<bool, Report> {
        if let Some(state) = &self.state {
            Ok(!state.is_provisioned())
        } else {
            Ok(true)
        }
    }

    async fn provision(&mut self, gs: Arc<GrandSlam>) -> Result<(), Report> {
        self.get_client_info().await?;
        self.get_state(gs).await?;
        Ok(())
    }
}

impl RemoteV3AnisetteProvider {
    async fn get_state(&mut self, gs: Arc<GrandSlam>) -> Result<&mut AnisetteState, Report> {
        if self.state.is_none() {
            if let Ok(Some(state)) = &self.storage.retrieve_data("anisette_state") {
                if let Ok(state) = plist::from_bytes(state) {
                    info!("Loaded existing anisette state");
                    self.state = Some(state);
                } else {
                    warn!("Failed to parse existing anisette state, starting fresh");
                    self.state = Some(AnisetteState::new());
                }
            } else {
                info!("No existing anisette state found");
                self.state = Some(AnisetteState::new());
            }
        }

        let state = self.state.as_mut().ok_or_report()?;
        if !state.is_provisioned() {
            info!("Provisioning required...");
            Self::provision(state, gs, &self.url, self.websocket_proxy.as_deref())
                .await
                .context("Failed to provision")?;
        }
        let buf = Vec::new();
        let mut writer = std::io::BufWriter::new(buf);
        plist::to_writer_xml(&mut writer, &state)?;
        self.storage
            .store_data("anisette_state", &writer.into_inner()?)?;

        Ok(state)
    }

    async fn provisioning_headers(state: &AnisetteState) -> Result<HeaderMap, Report> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Apple-I-MD-LU",
            HeaderValue::from_str(&hex::encode(state.get_md_lu()))?,
        );
        // headers.insert(
        //     "X-Apple-I-Client-Time",
        //     HeaderValue::from_str(
        //         &Utc::now()
        //             .round_subsecs(0)
        //             .format("%+")
        //             .to_string()
        //             .replace("+00:00", "Z"),
        //     )?,
        // );
        // headers.insert("X-Apple-I-TimeZone", HeaderValue::from_static("UTC"));
        // headers.insert("X-Apple-Locale", HeaderValue::from_static("en_US"));
        headers.insert(
            "X-Mme-Device-Id",
            HeaderValue::from_str(&state.get_device_id())?,
        );

        Ok(headers)
    }
    async fn provision(
        state: &mut AnisetteState,
        gs: Arc<GrandSlam>,
        url: &str,
        proxy_url: Option<&str>,
    ) -> Result<(), Report> {
        let start_provisioning = gs.get_url("midStartProvisioning")?;
        let end_provisioning = gs.get_url("midFinishProvisioning")?;

        let websocket_url = format!("{}/v3/provisioning_session", url)
            .replace("https://", "wss://")
            .replace("http://", "ws://");

        debug!("Starting provisioning at {}", websocket_url);
        // let (mut ws_stream, _) = timeout(
        //     Duration::from_secs(30),
        //     tokio_tungstenite::connect_async(&websocket_url),
        // )
        // .await
        // .map_err(|_| {
        //     report!("Timed out connecting to provisioning socket. Try a different anisette server.")
        // })
        // .context("Failed to connect to provisioning socket")?
        // .context("Failed to connect to provisioning socket")?;

        let mut ws = AppWebSocket::connect(&websocket_url, proxy_url.as_deref())
            .await
            .context("Failed to connect to provisioning socket")?;

        debug!("Connected to provisioning socket");

        loop {
            let Some(msg) = ws.next().await else {
                bail!("Provisioning socket closed unexpectedly");
            };
            let msg = msg?;

            let text = match msg {
                WsMessage::Close => bail!("Provisioning socket closed unexpectedly"),
                WsMessage::Text(t) => t,
            };

            // Provisioning frames contain ADI/key material; never log their contents.
            debug!("Received provisioning message");
            let provision_msg: ProvisioningMessage = serde_json::from_str(&text)?;

            match provision_msg {
                ProvisioningMessage::GiveIdentifier => {
                    ws.send_text(
                        serde_json::json!({
                            "identifier": BASE64_STANDARD.encode(state.keychain_identifier),
                        })
                        .to_string()
                        .into(),
                    )
                    .await
                    .context("Failed to send identifier")?;
                }
                ProvisioningMessage::GiveStartProvisioningData => {
                    let body = plist!(dict {
                        "Header": {},
                        "Request": {}
                    });

                    let response = gs
                        .plist_request(
                            &start_provisioning,
                            &body,
                            Some(Self::provisioning_headers(state).await?),
                            None,
                        )
                        .await
                        .context("Failed to send start provisioning request")?;

                    let spim = response
                        .get_str("spim")
                        .context("Start provisioning response missing spim")?;

                    ws.send_text(
                        serde_json::json!({
                            "spim": spim,
                        })
                        .to_string()
                        .into(),
                    )
                    .await
                    .context("Failed to send start provisioning data")?;
                }
                ProvisioningMessage::GiveEndProvisioningData { cpim } => {
                    let body = plist!(dict {
                        "Header": {},
                        "Request": {
                            "cpim": cpim,
                        }
                    });

                    let response = gs
                        .plist_request(
                            &end_provisioning,
                            &body,
                            Some(Self::provisioning_headers(state).await?),
                            None,
                        )
                        .await
                        .context("Failed to send end provisioning request")?;

                    ws.send_text(
                        serde_json::json!({
                            "ptm": response
                                .get_str("ptm")
                                .context("End provisioning response missing ptm")?,
                            "tk": response
                                .get_str("tk")
                                .context("End provisioning response missing tk")?,
                        })
                        .to_string()
                        .into(),
                    )
                    .await
                    .context("Failed to send start provisioning data")?;
                }
                ProvisioningMessage::ProvisioningSuccess { adi_pb } => {
                    state.adi_pb = Some(BASE64_STANDARD.decode(adi_pb)?);
                    ws.close().await?;
                    info!("Provisioning successful");
                    break;
                }
                ProvisioningMessage::Timeout => bail!("Anisette provisioning timed out"),
                ProvisioningMessage::InvalidIdentifier => {
                    bail!("Anisette provisioning failed: invalid identifier")
                }
                ProvisioningMessage::StartProvisioningError { message } => {
                    return Err(
                        report!("Anisette provisioning failed: start provisioning error")
                            .attach(message),
                    );
                }
                ProvisioningMessage::EndProvisioningError { message } => {
                    return Err(
                        report!("Anisette provisioning failed: end provisioning error")
                            .attach(message),
                    );
                }
            }
        }

        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(tag = "result")]
enum ProvisioningMessage {
    GiveIdentifier,
    GiveStartProvisioningData,
    GiveEndProvisioningData { cpim: String },
    ProvisioningSuccess { adi_pb: String },
    Timeout,
    InvalidIdentifier,
    StartProvisioningError { message: String },
    EndProvisioningError { message: String },
}

#[derive(Deserialize)]
#[serde(tag = "result")]
enum AnisetteHeaders {
    GetHeadersError {
        message: String,
    },
    Headers {
        #[serde(rename = "X-Apple-I-MD-M")]
        machine_id: String,
        #[serde(rename = "X-Apple-I-MD")]
        one_time_password: String,
        #[serde(rename = "X-Apple-I-MD-RINFO")]
        routing_info: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::storage::InMemoryStorage;

    #[test]
    fn retries_dropped_post_without_changing_body_or_identity() {
        use std::io::{Read, Write};
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut bodies = Vec::new();
            for attempt in 0..2 {
                let (mut stream, _) = socket.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let (header_end, length) = loop {
                    let mut buffer = [0; 1024];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0, "request closed before headers");
                    request.extend_from_slice(&buffer[..count]);
                    assert!(request.len() < 4096);
                    if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        assert!(headers.starts_with("POST /v3/get_headers HTTP/1.1\r\n"));
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        break (end + 4, length);
                    }
                };
                while request.len() < header_end + length {
                    let mut buffer = [0; 1024];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0, "request closed before body");
                    request.extend_from_slice(&buffer[..count]);
                }
                bodies.push(request[header_end..header_end + length].to_vec());
                if attempt == 1 {
                    let body = r#"{"result":"Headers","X-Apple-I-MD-M":"fixture-mdm","X-Apple-I-MD":"fixture-otp","X-Apple-I-MD-RINFO":"17106176"}"#;
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            }
            assert_eq!(bodies[0], bodies[1]);
        });
        let mut provider = RemoteV3AnisetteProvider::new(
            &format!(" http://{address}/ "),
            Box::new(InMemoryStorage::new()),
            "0".into(),
        )
        .unwrap();
        provider.state = Some(AnisetteState {
            keychain_identifier: [0; 16],
            adi_pb: Some(Vec::new()),
        });
        let headers = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(provider.get_anisette_data())
            .unwrap()
            .get_headers();
        assert_eq!(headers["X-Apple-I-MD"], "fixture-otp");
        assert_eq!(provider.state.unwrap().keychain_identifier, [0; 16]);
        server.join().unwrap();
    }

    #[test]
    fn transport_error_keeps_cause_without_url_credentials() {
        crate::init().expect("error formatter installs");
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let mut provider = RemoteV3AnisetteProvider::new(
            &format!("https://fixture-user:fixture-password@127.0.0.1:{port}/?token=fixture-token"),
            Box::new(InMemoryStorage::new()),
            "0".into(),
        )
        .unwrap();
        provider.state = Some(AnisetteState {
            keychain_identifier: [0; 16],
            adi_pb: Some(Vec::new()),
        });
        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(provider.get_anisette_data())
            .unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("Failed to get anisette headers (v3 POST)"));
        assert!(rendered.contains("Caused by:"));
        for secret in ["fixture-user", "fixture-password", "fixture-token"] {
            assert!(!rendered.contains(secret), "request URL was not redacted");
        }
    }
}
