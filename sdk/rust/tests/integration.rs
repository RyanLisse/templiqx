use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use templiqx_adapter_rust::{
    CallOptions, CasCallOptions, Client, ClientOptions, TempliqxError,
    generated::{CompileRequest, CreatePackageRequest, ExecuteRequest, UpdatePackageRequest},
};
use tokio::{process::Command, time::sleep};

fn is_fingerprint(value: &str) -> bool {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[tokio::test]
#[ignore = "requires TEMPLIQX_SDK_IT=1 and boots the real HTTP server"]
async fn deterministic_fake_server_conformance() {
    if std::env::var("TEMPLIQX_SDK_IT").as_deref() != Ok("1") {
        eprintln!("skipped: set TEMPLIQX_SDK_IT=1");
        return;
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let packages = temp.path().join("packages");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&packages).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let base_url = format!("http://127.0.0.1:{port}");

    let mut server = Command::new("cargo")
        .args(["run", "--quiet", "-p", "templiqx-http-server"])
        .current_dir(&repo_root)
        .env_remove("MODEL_API_KEY")
        .env("TEMPLIQX_HTTP_ADDR", format!("127.0.0.1:{port}"))
        .env("TEMPLIQX_ROOT", &packages)
        .env("TEMPLIQX_WORKSPACE", &workspace)
        .env("TEMPLIQX_RUNTIME_MODE", "deterministic-fake")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("start templiqx-http-server");

    let probe = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(status) = server.try_wait().expect("server status") {
            panic!("templiqx-http-server exited during startup: {status}");
        }
        if probe
            .get(format!("{base_url}/operations/v1/health/ready"))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        assert!(Instant::now() < deadline, "server did not become ready");
        sleep(Duration::from_millis(100)).await;
    }

    let client = Client::new(
        &base_url,
        ClientOptions {
            timeout: Duration::from_secs(5),
            ..ClientOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        client
            .get_operations_v1_liveness(CallOptions::default())
            .await
            .unwrap()
            .data
            .status
            .to_string(),
        "ok"
    );
    assert_eq!(
        client
            .get_operations_v1_readiness(CallOptions::default())
            .await
            .unwrap()
            .data
            .status
            .to_string(),
        "ready"
    );
    let catalog = client.catalog(CallOptions::default()).await.unwrap();
    assert!(catalog.data.ok);
    assert!(
        catalog
            .data
            .result
            .iter()
            .any(|operation| operation == "execute_contract")
    );

    let created = client
        .create_package(
            &CreatePackageRequest {
                name: "sdk-rust-it".into(),
                version: "0.1.0".into(),
            },
            CallOptions::default(),
        )
        .await
        .unwrap();
    let package_fingerprint = created.data.fingerprints.get("package").cloned().unwrap();
    assert!(is_fingerprint(&package_fingerprint));
    let updated = client
        .update_package(
            "sdk-rust-it",
            &UpdatePackageRequest {
                description: Some("Rust SDK integration".into()),
                version: None,
            },
            CasCallOptions {
                timeout: None,
                request_id: None,
                if_match: package_fingerprint,
            },
        )
        .await
        .unwrap();
    assert!(updated.data.ok);

    let contract_source =
        fs::read_to_string(repo_root.join("examples/packages/demo/contracts/greeting.yaml"))
            .unwrap();
    let put = client
        .put_contract(
            "sdk-rust-it",
            "greeting",
            &contract_source,
            CallOptions::default(),
        )
        .await
        .unwrap();
    assert!(put.data.ok);

    let compile: CompileRequest = serde_json::from_value(serde_json::json!({
        "render": {
            "inputs": {"name": "Ryan"},
            "context": {"organization": "Blinqx"}
        },
        "capabilities": ["structured_output"]
    }))
    .unwrap();
    let compiled = client
        .compile_contract("sdk-rust-it", "greeting", &compile, CallOptions::default())
        .await
        .unwrap();
    assert!(compiled.data.ok);

    let execute: ExecuteRequest = serde_json::from_value(serde_json::json!({
        "render": {
            "inputs": {"name": "Ryan"},
            "context": {"organization": "Blinqx"}
        },
        "capabilities": ["structured_output"],
        "fixture_output": {"greeting": "Hello Ryan"},
        "stream": false
    }))
    .unwrap();
    let executed = client
        .execute_contract("sdk-rust-it", "greeting", &execute, CallOptions::default())
        .await
        .unwrap();
    let receipt_fingerprint = &executed.data.result.as_ref().unwrap().output_fingerprint;
    assert!(is_fingerprint(receipt_fingerprint));
    println!("ExecutionReceipt fingerprint: {receipt_fingerprint}");

    let error = client
        .inspect_contract("missing", "greeting", CallOptions::default())
        .await
        .expect_err("missing contract should be an HTTP error");
    match error {
        TempliqxError::Http(error) => {
            assert_eq!(error.status.as_u16(), 404);
            assert_eq!(error.envelope.unwrap().diagnostics[0].code, "TQX_NOT_FOUND");
        }
        other => panic!("expected HTTP error, got {other:?}"),
    }

    let timeout = client
        .catalog(CallOptions {
            timeout: Some(Duration::from_nanos(1)),
            request_id: Some("sdk-rust-it-timeout".into()),
            ..CallOptions::default()
        })
        .await
        .expect_err("one-nanosecond request should time out");
    match timeout {
        TempliqxError::Transport(error) => {
            assert_eq!(error.request_id, "sdk-rust-it-timeout");
            assert!(error.is_timeout());
        }
        other => panic!("expected transport error, got {other:?}"),
    }

    server.kill().await.expect("stop server");
}
