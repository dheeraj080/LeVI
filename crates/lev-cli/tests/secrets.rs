// SPDX-License-Identifier: MIT OR Apache-2.0
//! `levi secret`, through the **real binary**.
//!
//! Two properties are worth spawning a process for. The first: a value never
//! appears in an argument, in stdout, or anywhere the command did not deliberately
//! put it — an argument would sit in the shell history and in `ps` output. The
//! second: a keychain secret is read when it is needed, not when a file is parsed.
//! The round-trip needs an actual credential store, so it is `#[ignore]`d and gated:
//! `LEVI_KEYCHAIN_TEST=1 cargo test -p lev-cli --test secrets -- --ignored` runs it
//! wherever a credential store exists.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A fresh, unique directory for one test's collection.
fn scratch_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("levi-secret-{label}-{nanos}"));
    std::fs::create_dir_all(dir.join("requests")).expect("create scratch dir");
    dir
}

fn write_collection(dir: &Path, base_url: &str, secret_name: &str) {
    std::fs::write(
        dir.join("collection.yaml"),
        format!(
            "id: scratch\nname: Scratch\nbaseUrl: {base_url}\n\
             vars:\n  tok:\n    secret: {secret_name}\n"
        ),
    )
    .expect("write collection");
}

fn write_guarded_request(dir: &Path) {
    std::fs::write(
        dir.join("requests").join("guarded.yaml"),
        "id: guarded\nname: Guarded\nmethod: GET\nurl: /guarded\n\
         headers:\n  - name: X-From-Secret\n    value: \"{{tok}}\"\n",
    )
    .expect("write request");
}

/// Run `levi secret <args...>`, feeding `stdin` the value; returns the output.
async fn secret_command(args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_levi"));
    command
        .args(std::iter::once("secret").chain(args.iter().copied()))
        .env_remove("LEVI_LOG")
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn `levi secret`");
    let mut value = child.stdin.take().expect("child stdin");
    if let Some(stdin) = stdin {
        // A command that refuses its arguments exits without reading, so a broken
        // pipe here is one of the expected outcomes, not a failure. The exit code
        // and stderr say what actually happened.
        let _ = value.write_all(stdin.as_bytes()).await;
    }
    drop(value);
    child
        .wait_with_output()
        .await
        .expect("wait for `levi secret`")
}

#[tokio::test]
async fn storing_a_secret_needs_a_name_and_something_to_store() {
    // Every one of these is refused *before* the keychain is consulted, so they are
    // the same on a machine with a credential store and a machine without one — and
    // none of them can leave a half-stored secret behind.
    let no_args = secret_command(&[], None).await;
    assert_eq!(no_args.status.code(), Some(2), "{no_args:?}");

    let no_name = secret_command(&["set"], Some("value")).await;
    assert_eq!(no_name.status.code(), Some(2), "{no_name:?}");

    let empty = secret_command(&["set", "token"], Some("")).await;
    assert_eq!(empty.status.code(), Some(2), "{empty:?}");
    let stderr = String::from_utf8_lossy(&empty.stderr);
    assert!(stderr.contains("empty value"), "{stderr}");

    // Just a newline is still empty: that is the shell's line ending, not a value.
    let only_newline = secret_command(&["set", "token"], Some("\n")).await;
    assert_eq!(only_newline.status.code(), Some(2), "{only_newline:?}");

    let spaced = secret_command(&["set", "two words"], Some("value")).await;
    assert_eq!(spaced.status.code(), Some(2), "{spaced:?}");

    let unknown = secret_command(&["forget", "token"], Some("value")).await;
    assert_eq!(unknown.status.code(), Some(2), "{unknown:?}");

    for output in [&no_args, &no_name, &empty, &only_newline, &spaced, &unknown] {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("value"),
            "a value leaked into stdout: {stdout}"
        );
    }
}

#[tokio::test]
#[ignore = "needs a real OS keychain: LEVI_KEYCHAIN_TEST=1 cargo test -- --ignored"]
async fn a_keychain_secret_is_read_back_at_send_time() {
    // The full loop, against a real credential store: store a value with the CLI,
    // then send a request whose header references it — with no `LEVI_SECRET_*` in
    // the environment, so the keychain is the only place the value can come from.
    // Ignored by default because CI containers usually have no credential store;
    // run it anywhere one exists.
    if std::env::var_os("LEVI_KEYCHAIN_TEST").is_none() {
        eprintln!("skipping: set LEVI_KEYCHAIN_TEST=1 to run the keychain round-trip");
        return;
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("levi-ci-probe-{nanos}");
    let value = "s3cr3t-from-the-keychain";

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/guarded"))
        .and(header("x-from-secret", value))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&server)
        .await;

    let dir = scratch_dir("round-trip");
    write_collection(&dir, &server.uri(), &name);
    write_guarded_request(&dir);

    let stored = secret_command(&["set", &name], Some(value)).await;
    assert!(stored.status.success(), "storing failed: {stored:?}");

    // The request itself goes through the whole engine — prepare, resolve, send —
    // so a header that arrives with the right value proves the resolution happened.
    let request = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "requests/guarded.yaml"])
        .env_remove("LEVI_LOG")
        .env_remove("RUST_LOG")
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi request");
    let stdout = String::from_utf8_lossy(&request.stdout);
    assert!(
        stdout.contains("HTTP 200"),
        "the keychain secret did not reach the target: {request:?}"
    );

    let removed = secret_command(&["delete", &name], None).await;
    assert!(removed.status.success(), "cleanup failed: {removed:?}");

    // The value must not survive in the result, whatever the engine echoed back.
    assert!(!stdout.contains(value), "the secret leaked into stdout");
}
