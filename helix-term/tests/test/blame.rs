use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use helix_term::application::Application;
use helix_view::editor::InlineBlameConfig;

use super::*;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=helix-test",
            "-c",
            "user.email=test@helix.org",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .expect("failed to run git");
    assert!(
        output.status.success(),
        "`git {args:?}` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo_with_files(names: &[&str]) -> anyhow::Result<(tempfile::TempDir, Vec<PathBuf>)> {
    let tmp = tempfile::tempdir()?;
    // Resolve symlinks (e.g. /var on macOS) so paths match what git reports.
    let dir = std::fs::canonicalize(tmp.path())?;
    git(&dir, &["init"]);
    let paths: Vec<_> = names
        .iter()
        .map(|name| {
            let path = dir.join(name);
            std::fs::write(&path, format!("contents of {name}\n")).unwrap();
            path
        })
        .collect();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-m", "init"]);
    Ok((tmp, paths))
}

fn app_with_auto_fetch_blame(files: &[PathBuf]) -> anyhow::Result<Application> {
    let mut builder = AppBuilder::new().with_config(Config {
        editor: helix_view::editor::Config {
            inline_blame: InlineBlameConfig {
                auto_fetch: true,
                ..Default::default()
            },
            ..test_editor_config()
        },
        ..test_config()
    });
    for file in files {
        builder = builder.with_file(file, None);
    }
    builder.build()
}

async fn run_until(app: &mut Application, done: impl Fn(&Application) -> bool) {
    // Timeout guards against hanging when a blame result never arrives.
    let wait = async {
        while !done(app) {
            run_event_loop_until_idle(app).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("timed out waiting for blame");
}

#[tokio::test(flavor = "multi_thread")]
async fn blame_is_computed_for_every_opened_document() -> anyhow::Result<()> {
    let (_repo, files) = repo_with_files(&["a.txt", "b.txt", "c.txt"])?;
    let mut app = app_with_auto_fetch_blame(&files)?;

    run_until(&mut app, |app| {
        app.editor
            .documents()
            .all(|doc| matches!(doc.file_blame, Some(Ok(_))))
    })
    .await;

    assert_eq!(app.editor.documents().count(), 3);
    test_key_sequence(&mut app, Some(":qa!<ret>"), None, true).await
}

#[tokio::test(flavor = "multi_thread")]
async fn blame_for_closed_or_renamed_document_is_dropped() -> anyhow::Result<()> {
    let (repo, files) = repo_with_files(&["a.txt", "b.txt", "c.txt"])?;
    let mut app = app_with_auto_fetch_blame(&files)?;
    let ids: Vec<_> = files
        .iter()
        .map(|file| app.editor.document_id_by_path(file).unwrap())
        .collect();

    // Callbacks apply only on the event loop, so this runs first.
    assert!(app.editor.close_document(ids[0], true).is_ok());
    app.editor
        .document_mut(ids[1])
        .unwrap()
        .set_path(Some(&repo.path().join("renamed.txt")));

    // Walks run in id order, so earlier dispatches precede the last.
    run_until(&mut app, |app| {
        matches!(app.editor.document(ids[2]).unwrap().file_blame, Some(Ok(_)))
    })
    .await;

    assert!(app.editor.document(ids[0]).is_none());
    assert!(app.editor.document(ids[1]).unwrap().file_blame.is_none());
    test_key_sequence(&mut app, Some(":qa!<ret>"), None, true).await
}
