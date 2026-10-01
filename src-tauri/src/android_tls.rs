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
                _ => None,
            }
        })
    });
    matches!(result, Ok(Some(())))
}
