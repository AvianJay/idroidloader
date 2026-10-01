use std::{future::Future, net::IpAddr, pin::Pin, time::Duration};

use idevice::{
    Idevice, IdeviceError, IdeviceService, RsdService,
    lockdown::LockdownClient,
    pairing_file::PairingFile,
    provider::{IdeviceProvider, TcpProvider, UsbmuxdProvider},
};
use tauri::{AppHandle, State};
use tauri_plugin_fs::FilePath;
use tokio_util::sync::CancellationToken;

use crate::{
    device::{DeviceInfo, DeviceInfoMutex, DeviceInfoWithPairing, PairingCancelToken},
    error::AppError,
    input_file::read_pairing_input,
};

pub enum DeviceProvider {
    Usb(UsbmuxdProvider),
    Tcp(TcpProvider),
    Remote(std::sync::Arc<crate::wireless::RemoteConnection>),
}

// PairingFile contains private keys. Never delegate Debug to TcpProvider.
impl std::fmt::Debug for DeviceProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Usb(_) => "UsbProvider",
            Self::Tcp(_) => "TcpProvider",
            Self::Remote(_) => "RemoteProvider",
        })
    }
}

impl IdeviceProvider for DeviceProvider {
    fn connect(
        &self,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<Idevice, IdeviceError>> + Send>> {
        let connection = match self {
            Self::Usb(provider) => provider.connect(port),
            Self::Tcp(provider) => provider.connect(port),
            Self::Remote(_) => Box::pin(async { Err(IdeviceError::ServiceNotFound) }),
        };
        Box::pin(async move {
            tokio::time::timeout(Duration::from_secs(15), connection)
                .await
                .map_err(|_| {
                    IdeviceError::Socket(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Device connection timed out",
                    ))
                })?
        })
    }
    fn label(&self) -> &str {
        match self {
            Self::Usb(provider) => provider.label(),
            Self::Tcp(provider) => provider.label(),
            Self::Remote(_) => "iDroidLoader Wireless",
        }
    }
    fn get_pairing_file(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PairingFile, IdeviceError>> + Send>> {
        match self {
            Self::Usb(provider) => provider.get_pairing_file(),
            Self::Tcp(provider) => provider.get_pairing_file(),
            Self::Remote(_) => Box::pin(async { Err(IdeviceError::ServiceNotFound) }),
        }
    }
}

impl DeviceProvider {
    pub async fn service<T: IdeviceService + RsdService>(&self) -> Result<T, IdeviceError> {
        match self {
            Self::Remote(remote) => remote.service::<T>().await,
            _ => T::connect(self).await,
        }
    }

    pub async fn install_signed(&self, path: &std::path::Path) -> Result<(), rootcause::Report> {
        match self {
            Self::Remote(remote) => remote.install_signed(path).await,
            _ => isideload::sideload::install::install_app(self, path, |_| {}).await,
        }
    }
}

fn parse_address(address: &str) -> Result<(IpAddr, Option<u32>), AppError> {
    let address = address.trim();
    let (ip, scope) = if let Some((ip, scope)) = address.rsplit_once('%') {
        let scope = scope.parse::<u32>().map_err(|_| {
            AppError::DeviceComs("IPv6 scope must be a numeric interface index".into())
        })?;
        (ip, Some(scope))
    } else {
        (address, None)
    };
    let ip: IpAddr = ip.parse().map_err(|_| {
        AppError::DeviceComs("Enter an IPv4 or IPv6 address without a port or URL prefix".into())
    })?;
    if ip.is_unspecified() || ip.is_multicast() || (scope.is_some() && ip.is_ipv4()) {
        return Err(AppError::DeviceComs(
            "Enter the iPhone's unicast IP address".into(),
        ));
    }
    Ok((ip, scope))
}

fn validate_lockdown_plist(bytes: &[u8]) -> Result<(), AppError> {
    let value = plist::Value::from_reader(std::io::Cursor::new(bytes)).map_err(|_| {
        AppError::LockdownPairing(
            "Invalid pairing file".into(),
            "Select an XML or binary plist pairing file".into(),
        )
    })?;
    let dict = value.as_dictionary().ok_or_else(|| {
        AppError::LockdownPairing(
            "Invalid pairing file".into(),
            "Expected a plist dictionary".into(),
        )
    })?;
    if !dict.contains_key("HostPrivateKey") && dict.contains_key("private_key") {
        return Err(AppError::LockdownPairing("This is a RemotePairing-only file".into(),
            "Import a Lockdown pairing file or iloader's combined pairing file for this connection mode".into()));
    }
    for field in [
        "DeviceCertificate",
        "HostCertificate",
        "HostPrivateKey",
        "RootCertificate",
        "RootPrivateKey",
    ] {
        if dict
            .get(field)
            .and_then(|v| v.as_data())
            .is_none_or(|v| v.is_empty())
        {
            return Err(AppError::LockdownPairing(
                "Incomplete Lockdown pairing file".into(),
                format!("Missing or invalid {field}"),
            ));
        }
    }
    for field in ["HostID", "SystemBUID", "WiFiMACAddress"] {
        if dict
            .get(field)
            .and_then(|v| v.as_string())
            .is_none_or(|v| v.is_empty())
        {
            return Err(AppError::LockdownPairing(
                "Incomplete Lockdown pairing file".into(),
                format!("Missing or invalid {field}"),
            ));
        }
    }
    Ok(())
}

pub fn tcp_provider(address: &str, bytes: &[u8]) -> Result<TcpProvider, AppError> {
    let (addr, scope_id) = parse_address(address)?;
    validate_lockdown_plist(bytes)?;
    let pairing_file = PairingFile::from_bytes(bytes).map_err(|_| {
        AppError::LockdownPairing(
            "Invalid pairing credentials".into(),
            "Certificates or private keys could not be decoded".into(),
        )
    })?;
    validate_pairing_credentials(&pairing_file)?;
    Ok(TcpProvider {
        addr,
        scope_id,
        pairing_file,
        label: "iDroidLoader".into(),
    })
}

// idevice decodes PEM without checking its DER contents, and its TLS setup can
// panic on an invalid client key. Validate using the same crypto backend first.
fn validate_pairing_credentials(pairing: &PairingFile) -> Result<(), AppError> {
    use rustls::pki_types::{PrivateKeyDer, pem::PemObject};
    let invalid = || {
        AppError::LockdownPairing(
            "Invalid pairing credentials".into(),
            "Certificates or private keys could not be decoded or do not match".into(),
        )
    };
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(pairing.root_certificate.clone())
        .map_err(|_| invalid())?;
    roots
        .add(pairing.device_certificate.clone())
        .map_err(|_| invalid())?;
    let key = PrivateKeyDer::from_pem_slice(&pairing.host_private_key).map_err(|_| invalid())?;
    rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| invalid())?
    .with_root_certificates(roots)
    .with_client_auth_cert(vec![pairing.host_certificate.clone()], key)
    .map_err(|_| invalid())?;
    Ok(())
}

#[tauri::command]
pub async fn connect_network_device(
    app: AppHandle,
    device_state: State<'_, DeviceInfoMutex>,
    cancel_state: State<'_, PairingCancelToken>,
    address: String,
    pairing_path: FilePath,
) -> Result<DeviceInfo, AppError> {
    // Check the address before invoking the document provider.
    parse_address(&address)?;
    let token = CancellationToken::new();
    {
        let mut guard = cancel_state.lock().unwrap();
        if let Some(old) = guard.replace(token.clone()) {
            old.cancel();
        }
    }
    let query = async {
        let file_app = app.clone();
        let bytes =
            tokio::task::spawn_blocking(move || read_pairing_input(&file_app, pairing_path))
                .await
                .map_err(|_| {
                    AppError::Filesystem("Pairing import failed".into(), String::new())
                })??;
        if crate::wireless::is_remote_record(&bytes)? {
            let _network = crate::wireless::WirelessNetworkGuard::acquire(&app)?;
            return crate::wireless::connect_record(bytes, Some(&address)).await;
        }
        let provider = DeviceProvider::Tcp(tcp_provider(&address, &bytes)?);
        let mut client = LockdownClient::connect(&provider).await.map_err(|_| {
            AppError::DeviceComs("Cannot reach iPhone. Check its IP, Wi-Fi debugging, and network isolation; unlock the iPhone and try again".into())
        })?;
        let pairing = provider
            .get_pairing_file()
            .await
            .map_err(|_| AppError::DeviceComs("Unable to load pairing credentials".into()))?;
        client.start_session(&pairing).await.map_err(|_| {
            AppError::DeviceComs("iPhone rejected the pairing file. Unlock it and check that the pairing is still trusted".into())
        })?;
        let mut values = Vec::new();
        for key in ["DeviceName", "ProductVersion", "UniqueDeviceID"] {
            let value = client
                .get_value(Some(key), None)
                .await
                .map_err(|_| AppError::DeviceComs(format!("Unable to read {key} from iPhone")))?;
            values.push(
                value
                    .as_string()
                    .ok_or_else(|| AppError::DeviceComs(format!("iPhone returned invalid {key}")))?
                    .to_owned(),
            );
        }
        let udid = values.pop().unwrap();
        if pairing
            .udid
            .as_ref()
            .is_some_and(|expected| expected != &udid)
        {
            return Err(AppError::DeviceComs(
                "Pairing file belongs to another iPhone".into(),
            ));
        }
        let info = DeviceInfo {
            name: values.remove(0),
            version: values.remove(0),
            udid,
            id: 0,
            connection_type: "Network".into(),
            address: Some(address.trim().to_owned()),
        };
        Ok(DeviceInfoWithPairing {
            info,
            pairing: bytes,
            remote: None,
        })
    };
    let result = tokio::select! {
        _ = token.cancelled() => Err(AppError::Canceled("Connection".into())),
        result = tokio::time::timeout(Duration::from_secs(40), query) => result.unwrap_or_else(|_| {
            Err(AppError::DeviceComs("Connection timed out. Check the IP and enable Wi-Fi debugging on iPhone before trying again".into()))
        }),
    };
    // Hold the token guard while committing so an obsolete attempt cannot replace a newer one.
    let mut guard = cancel_state.lock().unwrap();
    if token.is_cancelled() {
        return Err(AppError::Canceled("Connection".into()));
    }
    *guard = None;
    let selected = result?;
    let info = selected.info.clone();
    *device_state.lock().unwrap() = Some(selected);
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ip_addresses_and_ipv6_scope() {
        assert!(parse_address(" 192.168.1.2 ").is_ok());
        assert_eq!(parse_address("fe80::1%7").unwrap().1, Some(7));
    }
    #[test]
    fn rejects_urls_ports_and_non_device_addresses() {
        for address in [
            "https://192.168.1.2",
            "192.168.1.2:62078",
            "0.0.0.0",
            "ff02::1",
            "192.168.1.2%7",
            "fe80::1%wlan0",
        ] {
            assert!(parse_address(address).is_err(), "{address}");
        }
    }
    #[test]
    fn distinguishes_remote_only_files_without_echoing_their_contents() {
        let bytes = b"<plist version=\"1.0\"><dict><key>private_key</key><data>c2VjcmV0</data></dict></plist>";
        let error = validate_lockdown_plist(bytes).unwrap_err().to_string();
        assert!(error.contains("RemotePairing-only"));
        assert!(!error.contains("c2VjcmV0"));
    }
    #[test]
    fn rejects_malformed_and_incomplete_pairing() {
        assert!(validate_lockdown_plist(b"not a plist").is_err());
        assert!(validate_lockdown_plist(b"<plist version=\"1.0\"><dict/></plist>").is_err());
    }

    #[test]
    fn accepts_binary_combined_pairing_structure() {
        let mut dict = plist::Dictionary::new();
        for field in [
            "DeviceCertificate",
            "HostCertificate",
            "HostPrivateKey",
            "RootCertificate",
            "RootPrivateKey",
        ] {
            dict.insert(field.into(), plist::Value::Data(vec![1]));
        }
        for field in ["HostID", "SystemBUID", "WiFiMACAddress"] {
            dict.insert(field.into(), plist::Value::String("test".into()));
        }
        dict.insert("private_key".into(), plist::Value::Data(vec![1]));
        let mut bytes = Vec::new();
        plist::to_writer_binary(&mut bytes, &dict).unwrap();
        assert!(validate_lockdown_plist(&bytes).is_ok());
        // Structurally complete records with invalid certificates must still be rejected.
        assert!(tcp_provider("192.168.1.2", &bytes).is_err());
    }
}
