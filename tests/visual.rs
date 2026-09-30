//! Pixel screenshots of the `--demo` UI in Chromium (see `ui/mod.rs`),
//! compared with the PNGs in `tests/screenshots/`.
//!
//! They need Playwright's Chromium, so plain `cargo test` skips them:
//! `make visual` runs them and `make visual-update` rewrites the baselines.
//! Baselines are rendered on CI's Linux runner (fonts and antialiasing differ
//! between machines): take them from the `screenshots` artifact of the CI
//! workflow, run by hand with "update screenshots" ticked.

mod common;
mod ui;

use std::path::PathBuf;

use playwright_rs::protocol::Page;
use playwright_rs::{Animations, ScreenshotAssertionOptions, expect, expect_page};
use ui::open;

/// Compares the viewport with `tests/screenshots/{name}.png`. A missing
/// baseline fails rather than being written, unless `UPDATE_SNAPSHOTS` is set;
/// the capture is kept next to it as `{name}-actual.png` either way.
async fn assert_screenshot(page: &Page, name: &str) {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "screenshots"]
        .iter()
        .collect();
    let baseline = dir.join(format!("{name}.png"));
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let options = |update| {
        ScreenshotAssertionOptions::builder()
            .animations(Animations::Disabled)
            .update_snapshots(update)
            .build()
    };
    if !update && !baseline.exists() {
        let actual = dir.join(format!("{name}-actual.png"));
        let _ = expect_page(page)
            .to_have_screenshot(&actual, Some(options(true)))
            .await;
        panic!(
            "no baseline at {}; the capture is in {}",
            baseline.display(),
            actual.display()
        );
    }
    if let Err(e) = expect_page(page)
        .to_have_screenshot(&baseline, Some(options(update)))
        .await
    {
        panic!("{name}: {e}");
    }
}

#[tokio::test]
#[ignore = "needs a Playwright browser: make visual"]
async fn keys_table() {
    let ui = open("/", &[]).await;
    assert_screenshot(&ui.page, "keys").await;
}

#[tokio::test]
#[ignore = "needs a Playwright browser: make visual"]
async fn keys_table_dark_violet() {
    let ui = open(
        "/",
        &[("clavium_theme", "dark"), ("clavium_palette", "violet")],
    )
    .await;
    assert_screenshot(&ui.page, "keys-dark-violet").await;
}

#[tokio::test]
#[ignore = "needs a Playwright browser: make visual"]
async fn detail_dark() {
    let ui = open("/keys/renovate-github", &[("clavium_theme", "dark")]).await;
    assert_screenshot(&ui.page, "detail-dark").await;
}

#[tokio::test]
#[ignore = "needs a Playwright browser: make visual"]
async fn detail_on_demand() {
    let ui = open("/keys/github-repo-migration", &[]).await;
    assert_screenshot(&ui.page, "detail-on-demand").await;
}

#[tokio::test]
#[ignore = "needs a Playwright browser: make visual"]
async fn rotate_dialog() {
    let ui = open("/keys/renovate-github", &[]).await;
    ui.page
        .locator("[data-dialog-open=rotate-dialog]")
        .click(None)
        .await
        .unwrap();
    expect(ui.page.locator("#rotate-title"))
        .to_be_visible()
        .await
        .unwrap();
    assert_screenshot(&ui.page, "rotate").await;
}

#[tokio::test]
#[ignore = "needs a Playwright browser: make visual"]
async fn filtered_by_state() {
    let ui = open("/", &[]).await;
    ui.page
        .locator(".state-chip[data-state=expired]")
        .click(None)
        .await
        .unwrap();
    // Wait for htmx to swap in the filtered table and the updated chips.
    expect(
        ui.page
            .locator(".state-chip[data-state=expired][data-active=true]"),
    )
    .to_be_visible()
    .await
    .unwrap();
    assert_screenshot(&ui.page, "filtered-expired").await;
}

#[tokio::test]
#[ignore = "needs a Playwright browser: make visual"]
async fn command_palette() {
    let ui = open("/", &[]).await;
    ui.page.keyboard().press("Control+k", None).await.unwrap();
    expect(ui.page.locator("#command-results [role=option]").first())
        .to_be_visible()
        .await
        .unwrap();
    assert_screenshot(&ui.page, "command-palette").await;
}
