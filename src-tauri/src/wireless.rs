//! Device-initiated iOS 27 pairing and authenticated, userspace RSD tunnels.
//! Pairing records and PINs are never logged or returned as device metadata.
use std::{
    net::{IpAddr, SocketAddr, SocketAddrV6},
    path::Path,
    sync::Arc,
    time::Duration,
};

use idevice::{
    IdeviceError, RsdService,
    lockdown::LockdownClient,
    remote_pairing::{
        PAIRABLE_HOST_SERVICE_TYPE, PairableHost, PairableHostInfo, PeerDevice,
        RemotePairingClient, RpPairingFile, RpPairingSocket, connect_tls_psk_tunnel_native,
    },
    rsd::RsdHandshake,
    tcp::{adapter::Adapter, handle::AdapterHandle},
};
use mdns_sd::{DaemonEvent, ResolvedService, ScopedIp, ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use tauri::{AppHandle, State, ipc::Channel};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

use crate::{
    device::{DeviceInfo, DeviceInfoMutex, DeviceInfoWithPairing, PairingCancelToken},
    error::AppError,
};

const REMOTE_SERVICE: &str = "_remotepairing._tcp.local.";
const HOST_NAME: &str = "iDroidLoader";

#[derive(Default)]
pub struct WirelessAttempts(std::sync::Mutex<Attempts>);
#[derive(Default)]
struct Attempts {
    active: Option<(String, CancellationToken)>,
    canceled: std::collections::VecDeque<String>,
}
impl WirelessAttempts {
    fn begin(&self, id: &str) -> Result<CancellationToken, AppError> {
        let mut state = self.0.lock().unwrap();
        if state.canceled.iter().any(|canceled| canceled == id) {
            return Err(AppError::Canceled("Wireless pairing".into()));
        }
        let token = CancellationToken::new();
        if let Some((_, old)) = state.active.replace((id.into(), token.clone())) {
            old.cancel();
        }
        Ok(token)
    }
    fn finish(&self, id: &str) {
        let mut state = self.0.lock().unwrap();
        if state
            .active
            .as_ref()
            .is_some_and(|(active, _)| active == id)
        {
            state.active = None;
        }
    }
    fn cancel(&self, id: &str) {
        let mut state = self.0.lock().unwrap();
        if state
            .active
            .as_ref()
            .is_some_and(|(active, _)| active == id)
        {
            state.active.as_ref().unwrap().1.cancel();
        }
        if !state.canceled.iter().any(|canceled| canceled == id) {
            if state.canceled.len() == 32 {
                state.canceled.pop_front();
            }
            state.canceled.push_back(id.into());
        }
    }
}

#[tauri::command]
pub fn cancel_wireless_pairing(request_id: String, attempts: State<'_, WirelessAttempts>) {
    attempts.cancel(&request_id);
}

struct RemoteServices {
    handle: AdapterHandle,
    rsd: RsdHandshake,
}

pub struct RemoteConnection {
    services: Mutex<RemoteServices>,
    // Retain the authenticated control channel for the tunnel's lifetime.
    _control: Mutex<RemotePairingClient<RpPairingSocket<TcpStream>>>,
}

impl RemoteConnection {
    pub async fn sign_and_install(
        &self,
        sideloader: &mut isideload::sideload::sideloader::Sideloader<
            isideload::util::callbacks::MaxCertsCallbackBox,
        >,
        app_path: std::path::PathBuf,
        device: isideload::util::device::IdeviceInfo,
    ) -> Result<Option<isideload::sideload::application::SpecialApp>, rootcause::Report> {
        let mut services = self.services.lock().await;
        let RemoteServices { handle, rsd } = &mut *services;
        sideloader
            .install_app_rsd(
                handle,
                rsd,
                device,
                app_path,
                false,
                None::<fn(f32) -> std::future::Ready<()>>,
            )
            .await
    }

    pub async fn service<T: RsdService>(&self) -> Result<T, IdeviceError> {
        let mut services = self.services.lock().await;
        let RemoteServices { handle, rsd } = &mut *services;
        tokio::time::timeout(Duration::from_secs(20), rsd.connect::<T>(handle))
            .await
            .map_err(|_| IdeviceError::Socket(std::io::ErrorKind::TimedOut.into()))?
    }

    pub async fn install_signed(&self, path: &Path) -> Result<(), rootcause::Report> {
        let mut services = self.services.lock().await;
        let RemoteServices { handle, rsd } = &mut *services;
        isideload::sideload::install::install_app_rsd(handle, rsd, path, |_| {}).await
    }
}

// Channel messages are visible only in the active pairing UI, never tracing.
#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum WirelessStatus {
    Advertising { name: String },
    Pin { code: String },
    Connecting,
}

struct Mdns(ServiceDaemon);
impl Drop for Mdns {
    fn drop(&mut self) {
        let _ = self.0.shutdown();
    }
}

fn mdns() -> Result<Mdns, AppError> {
    ServiceDaemon::new()
        .map(Mdns)
        .map_err(|_| failure("Unable to start local network discovery"))
}

fn pairing_listener() -> Result<TcpListener, AppError> {
    // Advertised IPv6 addresses must be reachable too; iPhone may prefer them.
    let socket = socket2::Socket::new(
        socket2::Domain::IPV6,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .and_then(|socket| {
        socket.set_only_v6(false)?;
        socket.bind(&SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0)).into())?;
        socket.listen(4)?;
        socket.set_nonblocking(true)?;
        Ok(socket)
    })
    .map_err(|_| failure("Unable to start wireless pairing listener"))?;
    TcpListener::from_std(socket.into())
        .map_err(|_| failure("Unable to start wireless pairing listener"))
}

fn failure(message: &str) -> AppError {
    AppError::RemotePairing(message.into())
}

async fn advertise(
    info: &PairableHostInfo,
    record: &RpPairingFile,
    port: u16,
) -> Result<Mdns, AppError> {
    let daemon = mdns()?;
    daemon
        .0
        .set_service_name_len_max(30)
        .map_err(|_| failure("Unable to configure wireless discovery"))?;
    let records = info.mdns_txt_records(&record.identifier);
    let properties: Vec<_> = records
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let service = ServiceInfo::new(
        PAIRABLE_HOST_SERVICE_TYPE,
        &record.identifier,
        &format!("idroidloader-{}.local.", &record.identifier[..8]),
        "",
        port,
        &properties[..],
    )
    .map_err(|_| failure("Unable to create wireless advertisement"))?
    .enable_addr_auto();
    let events = daemon
        .0
        .monitor()
        .map_err(|_| failure("Unable to monitor wireless discovery"))?;
    daemon
        .0
        .register(service)
        .map_err(|_| failure("Unable to advertise on this Wi-Fi network"))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    loop {
        match tokio::time::timeout_at(deadline, events.recv_async()).await {
            Ok(Ok(DaemonEvent::Announce(_, _))) => break,
            Ok(Ok(DaemonEvent::Error(_))) => {
                return Err(failure("Unable to advertise on this Wi-Fi network"));
            }
            Ok(Ok(_)) => {}
            _ => {
                return Err(failure(
                    "No Wi-Fi interface could advertise iDroidLoader. Connect Android to Wi-Fi and try again",
                ));
            }
        }
    }
    Ok(daemon)
}

pub fn is_remote_record(bytes: &[u8]) -> Result<bool, AppError> {
    let value = plist::Value::from_reader(std::io::Cursor::new(bytes))
        .map_err(|_| failure("Invalid pairing file"))?;
    let dict = value
        .as_dictionary()
        .ok_or_else(|| failure("Invalid pairing file"))?;
    Ok(!dict.contains_key("HostPrivateKey") && dict.contains_key("private_key"))
}

fn parse_record(bytes: &[u8]) -> Result<RpPairingFile, AppError> {
    let record = RpPairingFile::from_bytes(bytes)
        .map_err(|_| failure("Invalid RemotePairing credentials"))?;
    if record.e_public_key != record.e_private_key.verifying_key()
        || record.alt_irk.as_ref().is_none_or(|key| key.len() != 16)
        || record.identifier.is_empty()
    {
        return Err(failure(
            "RemotePairing credentials are incomplete or do not match",
        ));
    }
    Ok(record)
}

fn matching_addresses(
    service: &ResolvedService,
    irk: &[u8],
    expected: Option<IpAddr>,
) -> Vec<SocketAddr> {
    let Some(identifier) = service.get_property_val_str("identifier") else {
        return vec![];
    };
    let Some(auth_tag) = service.get_property_val_str("authTag") else {
        return vec![];
    };
    if !PeerDevice::validate_auth_tag(irk, identifier, auth_tag) {
        return vec![];
    }
    service
        .addresses
        .iter()
        .filter(|ip| expected.is_none_or(|expected| ip.to_ip_addr() == expected))
        .map(|ip| match ip {
            ScopedIp::V6(ip) => SocketAddr::V6(SocketAddrV6::new(
                *ip.addr(),
                service.port,
                0,
                ip.scope_id().index,
            )),
            ip => SocketAddr::new(ip.to_ip_addr(), service.port),
        })
        .collect()
}

async fn find_device(
    irk: &[u8],
    expected: Option<IpAddr>,
) -> Result<(TcpStream, SocketAddr), AppError> {
    let daemon = mdns()?;
    let events = daemon
        .0
        .browse(REMOTE_SERVICE)
        .map_err(|_| failure("Unable to search for paired iPhone"))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        match tokio::time::timeout_at(deadline, events.recv_async()).await {
            Ok(Ok(ServiceEvent::ServiceResolved(service))) => {
                for address in matching_addresses(&service, irk, expected) {
                    let attempt = tokio::time::timeout_at(
                        deadline.min(tokio::time::Instant::now() + Duration::from_secs(3)),
                        TcpStream::connect(address),
                    )
                    .await;
                    if let Ok(Ok(stream)) = attempt {
                        return Ok((stream, address));
                    }
                }
            }
            Ok(Ok(_)) => {}
            _ => {
                return Err(failure(
                    "Paired iPhone was not found. Keep it unlocked on the same Wi-Fi; check network isolation and try again",
                ));
            }
        }
    }
}

pub async fn connect_record(
    bytes: Vec<u8>,
    expected_address: Option<&str>,
) -> Result<DeviceInfoWithPairing, AppError> {
    let mut record = parse_record(&bytes)?;
    let expected = expected_address
        .map(|address| {
            address
                .split('%')
                .next()
                .unwrap_or(address)
                .trim()
                .parse::<IpAddr>()
        })
        .transpose()
        .map_err(|_| failure("Invalid iPhone IP address"))?;
    let (stream, address) = find_device(record.alt_irk.as_deref().unwrap(), expected).await?;
    let mut control = RemotePairingClient::new(RpPairingSocket::new(stream), HOST_NAME);
    control
        .attempt_pair_verify()
        .await
        .map_err(|_| failure("Unable to negotiate RemotePairing"))?;
    // Do not fall back to pair-setup with a guessed/all-zero PIN.
    control.validate_pairing(&mut record).await.map_err(|_| {
        failure("iPhone rejected the RemotePairing record. Pair again from Developer Mode")
    })?;
    let tunnel_port = control
        .create_tcp_listener()
        .await
        .map_err(|_| failure("iPhone could not open an authenticated tunnel"))?;
    let mut tunnel_address = address;
    tunnel_address.set_port(tunnel_port);
    let stream = TcpStream::connect(tunnel_address)
        .await
        .map_err(|_| failure("Unable to connect to iPhone tunnel"))?;
    let tunnel = connect_tls_psk_tunnel_native(stream, control.encryption_key())
        .await
        .map_err(|_| failure("Unable to authenticate iPhone tunnel"))?;
    let client_ip = tunnel
        .info
        .client_address
        .parse()
        .map_err(|_| failure("Invalid tunnel address"))?;
    let server_ip = tunnel
        .info
        .server_address
        .parse()
        .map_err(|_| failure("Invalid tunnel address"))?;
    let rsd_port = tunnel.info.server_rsd_port;
    let mtu = usize::from(tunnel.info.mtu);
    if rsd_port == 0 || mtu < 1280 {
        return Err(failure("Invalid iPhone tunnel parameters"));
    }
    let mut adapter = Adapter::new(Box::new(tunnel.into_inner()), client_ip, server_ip);
    adapter.set_mss(mtu.saturating_sub(60));
    let mut handle = adapter.to_async_handle();
    let rsd = RsdHandshake::new(
        handle
            .connect(rsd_port)
            .await
            .map_err(|_| failure("Unable to connect to iPhone service discovery"))?,
    )
    .await
    .map_err(|_| failure("Unable to read iPhone services"))?;
    let remote = Arc::new(RemoteConnection {
        services: Mutex::new(RemoteServices { handle, rsd }),
        _control: Mutex::new(control),
    });
    let mut lockdown = remote
        .service::<LockdownClient>()
        .await
        .map_err(|_| failure("iPhone does not expose RemotePairing device information"))?;
    let mut values = Vec::new();
    for key in ["DeviceName", "ProductVersion", "UniqueDeviceID"] {
        let value = lockdown
            .get_value(Some(key), None)
            .await
            .map_err(|_| failure("Unable to read paired iPhone information"))?;
        values.push(
            value
                .as_string()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| failure("Invalid paired iPhone information"))?
                .to_owned(),
        );
    }
    let info = DeviceInfo {
        name: values.remove(0),
        version: values.remove(0),
        udid: values.remove(0),
        id: 0,
        connection_type: "Wireless".into(),
        address: Some(address.ip().to_string()),
    };
    Ok(DeviceInfoWithPairing {
        info,
        pairing: record.to_bytes(),
        remote: Some(remote),
    })
}

#[tauri::command]
pub async fn pair_wireless_device(
    app: AppHandle,
    device_state: State<'_, DeviceInfoMutex>,
    cancel_state: State<'_, PairingCancelToken>,
    attempts: State<'_, WirelessAttempts>,
    request_id: String,
    on_status: Channel<WirelessStatus>,
) -> Result<DeviceInfo, AppError> {
    let token = attempts.begin(&request_id)?;
    if let Some(old) = cancel_state.lock().unwrap().replace(token.clone()) {
        old.cancel();
    }
    let query = async {
        let _network = WirelessNetworkGuard::acquire(&app)?;
        let info = PairableHostInfo::generate(HOST_NAME, "Mac17,7");
        // Generate distinct identities for concurrently replaced attempts.
        let mut record = RpPairingFile::generate(
            &info
                .alt_irk
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        );
        let listener = pairing_listener()?;
        let _advertisement = advertise(
            &info,
            &record,
            listener
                .local_addr()
                .map_err(|_| failure("Unable to start wireless listener"))?
                .port(),
        )
        .await?;
        on_status
            .send(WirelessStatus::Advertising {
                name: HOST_NAME.into(),
            })
            .map_err(|_| AppError::Canceled("Wireless pairing".into()))?;
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|_| failure("Unable to accept iPhone pairing request"))?;
        let mut host = PairableHost::new(RpPairingSocket::new_device(stream), info);
        host.accept(&mut record, |code| async {
            let _ = on_status.send(WirelessStatus::Pin { code });
        })
        .await
        .map_err(|_| {
            failure("Wireless pairing failed. Check the PIN, unlock iPhone and start again")
        })?;
        on_status
            .send(WirelessStatus::Connecting)
            .map_err(|_| AppError::Canceled("Wireless pairing".into()))?;
        let selected = tokio::time::timeout(Duration::from_secs(40), connect_record(record.to_bytes(), None)).await
            .map_err(|_| failure("Paired successfully, but opening the iPhone tunnel timed out. Pair again and keep iPhone unlocked"))??;
        Ok::<_, AppError>(selected)
    };
    let result = tokio::select! {
        _ = token.cancelled() => Err(AppError::Canceled("Wireless pairing".into())),
        result = tokio::time::timeout(Duration::from_secs(180), query) => result.unwrap_or_else(|_| Err(failure("Wireless pairing timed out. Open Developer Mode on iOS 27 or newer and try again"))),
    };
    attempts.finish(&request_id);
    let mut guard = cancel_state.lock().unwrap();
    if token.is_cancelled() {
        return Err(AppError::Canceled("Wireless pairing".into()));
    }
    *guard = None;
    let selected = result?;
    let info = selected.info.clone();
    *device_state.lock().unwrap() = Some(selected);
    Ok(info)
}

// Scope Wi-Fi multicast and keep-screen-on to a pairing/discovery attempt.
pub struct WirelessNetworkGuard {
    #[cfg(target_os = "android")]
    app: AppHandle,
}
impl WirelessNetworkGuard {
    pub fn acquire(_app: &AppHandle) -> Result<Self, AppError> {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager;
            _app.state::<crate::android_wireless::AndroidWireless>()
                .acquire()?;
            Ok(Self { app: _app.clone() })
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(Self {})
        }
    }
}
impl Drop for WirelessNetworkGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager;
            self.app
                .state::<crate::android_wireless::AndroidWireless>()
                .release();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_before_start_and_stale_cancels_are_safe() {
        let attempts = WirelessAttempts::default();
        attempts.cancel("not-started");
        assert!(attempts.begin("not-started").is_err());
        let old = attempts.begin("old").unwrap();
        let current = attempts.begin("current").unwrap();
        assert!(old.is_cancelled());
        attempts.cancel("old");
        attempts.finish("old");
        assert!(!current.is_cancelled());
        attempts.cancel("current");
        assert!(current.is_cancelled());
    }
    #[test]
    fn incomplete_remote_records_are_rejected_without_contents() {
        let bytes = b"<plist version=\"1.0\"><dict><key>private_key</key><data>c2VjcmV0</data></dict></plist>";
        assert!(is_remote_record(bytes).unwrap());
        assert_eq!(
            parse_record(bytes).err().unwrap().to_string(),
            "Invalid RemotePairing credentials"
        );
    }
    #[test]
    fn fresh_unpaired_records_cannot_open_tunnels() {
        let record = RpPairingFile::generate("isolated-test");
        assert!(parse_record(&record.to_bytes()).is_err());
    }
    #[test]
    fn mismatched_remote_keys_are_rejected() {
        let mut record = RpPairingFile::generate("isolated-test");
        record.alt_irk = Some(vec![1; 16]);
        record.e_public_key = RpPairingFile::generate("other-test").e_public_key;
        assert!(parse_record(&record.to_bytes()).is_err());
    }
    #[test]
    fn discovery_rejects_unrelated_identities_and_filters_the_requested_ip() {
        let info = PairableHostInfo::generate("fixture-host", "Mac17,7");
        let identifier = "fixture-service";
        let properties = info.mdns_txt_records(identifier);
        let properties: Vec<_> = properties
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let service = ServiceInfo::new(
            REMOTE_SERVICE,
            identifier,
            "fixture.local.",
            "192.0.2.27",
            5000,
            &properties[..],
        )
        .unwrap()
        .as_resolved_service();
        assert!(matching_addresses(&service, &info.alt_irk, None).len() == 1);
        assert!(matching_addresses(&service, &[0; 16], None).is_empty());
        assert!(
            matching_addresses(&service, &info.alt_irk, Some("192.0.2.28".parse().unwrap()))
                .is_empty()
        );
    }

    #[tokio::test]
    async fn listener_accepts_ipv4_and_ipv6_and_closes_on_drop() {
        let listener = pairing_listener().unwrap();
        let port = listener.local_addr().unwrap().port();
        for address in [
            SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)),
            SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port)),
        ] {
            let (client, server) = tokio::join!(TcpStream::connect(address), listener.accept());
            assert!(client.is_ok() && server.is_ok());
        }
        drop(listener);
        assert!(
            TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
                .await
                .is_err()
        );
    }
}
