use jni::{
    EnvUnowned,
    objects::{JClass, JObject},
};

/// Called before Wry/Tauri starts any networking, with the application's class loader.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_idroidloader_mobile_NetworkTls_initialize<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    context: JObject<'caller>,
) {
    env.with_env(|env| rustls_platform_verifier::android::init_with_env(env, context))
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

// Instrumentation exercises the same GrandSlam builder that previously failed,
// without entering or sending an Apple ID/password. No test entry point in release APKs.
#[cfg(debug_assertions)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_idroidloader_mobile_TlsSmokeTest_probe(
    _env: EnvUnowned<'_>,
    _this: JObject<'_>,
    test: jni::sys::jint,
) -> jni::sys::jboolean {
    let result = std::panic::catch_unwind(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let runtime = tokio::runtime::Runtime::new().ok()?;
        runtime.block_on(async {
            let timeout = std::time::Duration::from_secs(20);
            match test {
                0 => {
                    let client =
                        isideload::auth::grandslam::GrandSlam::build_reqwest_client(false, None)
                            .ok()?;
                    client
                        .get("https://gsa.apple.com/grandslam/GsService2/lookup")
                        .timeout(timeout)
                        .send()
                        .await
                        .ok()
                        .map(|_| ())
                }
                1 => {
                    let client = reqwest::Client::builder().timeout(timeout).build().ok()?;
                    client
                        .get("https://ani.sidestore.io/v3/client_info")
                        .send()
                        .await
                        .ok()
                        .map(|_| ())
                }
                2 => {
                    let client =
                        isideload::auth::grandslam::GrandSlam::build_reqwest_client(false, None)
                            .ok()?;
                    let error = client
                        .get("https://expired.badssl.com/")
                        .timeout(timeout)
                        .send()
                        .await
                        .err()?;
                    // A connection/timeout error alone is not proof of certificate rejection.
                    let reason = format!("{error:?}");
                    reason.to_lowercase().contains("certificate").then_some(())
                }
                3 | 4 => {
                    let client = reqwest::Client::builder().timeout(timeout).build().ok()?;
                    let host = if test == 3 { "ani.sidestore.app" } else { "ani.sidestore.io" };
                    let response = client
                        .post(format!("https://{host}/v3/get_headers"))
                        // Public dummy data: exercise POST/TLS without account or provisioning secrets.
                        .json(&serde_json::json!({"identifier": "AAAAAAAAAAAAAAAAAAAAAA==", "adi_pb": ""}))
                        .send()
                        .await;
                    let response = match response {
                        Ok(response) => response,
                        Err(error) => {
                            // These fixed test URLs contain no credentials; the transport error
                            // does not include a request body or response headers.
                            eprintln!("Anisette dummy POST transport failure: {error:?}");
                            return None;
                        }
                    };
                    if !response.status().is_success() { return None; }
                    let body: serde_json::Value = response.json().await.ok()?;
                    (body.get("result")?.as_str()? == "GetHeadersError").then_some(())
                }
                5 => {
                    use isideload::{
                        anisette::{AnisetteProvider, remote_v3::RemoteV3AnisetteProvider},
                        auth::grandslam::GrandSlam,
                        util::storage::InMemoryStorage,
                    };
                    // Provision a fresh device in memory. No Apple account is used,
                    // and neither ADI data nor resulting headers are printed or saved.
                    let mut provider = RemoteV3AnisetteProvider::new(
                        "https://ani.sidestore.app/",
                        Box::new(InMemoryStorage::new()),
                        "0".into(),
                    ).ok()?;
                    let client_info = provider.get_client_info().await.ok()?;
                    let gs = match GrandSlam::new(client_info, false, None).await {
                        Ok(gs) => std::sync::Arc::new(gs),
                        Err(error) => return provisioning_probe_failure("lookup", error),
                    };
                    tokio::time::timeout(std::time::Duration::from_secs(90), async {
                        if let Err(error) = provider.provision(gs).await {
                            return provisioning_probe_failure("provisioning", error);
                        }
                        let data = match provider.get_anisette_data().await {
                            Ok(data) => data,
                            Err(error) => return provisioning_probe_failure("headers", error),
                        };
                        let headers = data.get_headers();
                        ["X-Apple-I-MD", "X-Apple-I-MD-M", "X-Mme-Device-Id"]
                            .iter()
                            .all(|key| headers.get(*key).is_some_and(|value| !value.is_empty()))
                            .then_some(())
                    }).await.ok()?
                }
                _ => None,
            }
        })
    });
    matches!(result, Ok(Some(())))
}

#[cfg(debug_assertions)]
fn provisioning_probe_failure(stage: &str, error: rootcause::Report) -> Option<()> {
    // Only static classifications escape the memory-only test: error attachments
    // may contain provisioning material and must never be printed.
    let rendered = error.to_string();
    let markers = [
        "Failed to connect to provisioning socket",
        "Failed to send start provisioning request",
        "Start provisioning response missing spim",
        "Failed to send end provisioning request",
        "Failed to get anisette headers",
        "Failed to get anisette client info",
        "Failed to fetch URL Bag",
        "Failed to parse URL Bag plist",
        "missing 'urls' dictionary",
        "Unable to find key in URL bag",
        "certificate",
        "timed out",
        "closed unexpectedly",
    ];
    let matched: Vec<_> = markers
        .into_iter()
        .filter(|marker| rendered.contains(marker))
        .collect();
    eprintln!("Anisette memory probe failed at {stage}; classifications: {matched:?}");
    for cause in error.iter_reports() {
        if let Some(isideload::SideloadError::AuthWithMessage(code, _)) =
            cause.downcast_current_context::<isideload::SideloadError>()
        {
            eprintln!("Anisette memory probe Apple error code: {code}");
        }
    }
    None
}
