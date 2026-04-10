use std::{fs, path::Path};

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;

pub async fn run(url: String, output: Option<String>) -> Result<(), String> {
    let html = fetch_html(&url).await?;
    let markdown = htmd::convert(&html).map_err(|e| format!("markdown conversion failed: {e}"))?;

    if markdown.trim().is_empty() {
        return Err("page rendered but produced no markdown content".to_string());
    }

    match output {
        Some(path) => {
            if let Some(parent) = Path::new(&path).parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create dirs: {e}"))?;
            }
            fs::write(&path, &markdown).map_err(|e| format!("failed to write file: {e}"))?;
            eprintln!("Written to {path}");
        }
        None => print!("{markdown}"),
    }

    Ok(())
}

async fn fetch_html(url: &str) -> Result<String, String> {
    let config = BrowserConfig::builder()
        .arg("--headless")
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .build()
        .map_err(|e| format!("browser config error: {e}"))?;

    let (mut browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|e| format!("failed to launch browser: {e}"))?;

    let handler_task = tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    let page = browser
        .new_page(url)
        .await
        .map_err(|e| format!("failed to navigate: {e}"))?;

    let html = page
        .wait_for_navigation()
        .await
        .map_err(|e| format!("navigation failed: {e}"))?
        .content()
        .await
        .map_err(|e| format!("failed to get page content: {e}"))?;

    browser.close().await.map_err(|e| format!("failed to close browser: {e}"))?;
    handler_task.await.map_err(|e| format!("handler task failed: {e}"))?;

    Ok(html)
}
