use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Set once `clock_settime` has succeeded. After that the system clock is the
/// time base, and the feed timestamp must not be added on top of it.
pub static CLOCK_SYNCED: AtomicBool = AtomicBool::new(false);

/// Periodically sync the system clock from an HTTP Date header.
/// Runs forever, syncing every 5 minutes. Errors are silently ignored
/// since the Caltrain feed timestamp provides a fallback.
pub async fn run_clock_sync() {
    loop {
        if sync_once().await.is_ok() {
            CLOCK_SYNCED.store(true, Ordering::Relaxed);
        }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

async fn sync_once() -> Result<(), Box<dyn std::error::Error>> {
    // Cloudflare answers plain HTTP with a 301 whose Date header is usable.
    // Following the redirect hits HTTPS, and this device's TLS stack fails
    // that handshake, so the clock would never be set.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let resp = client.head("http://one.one.one.one").send().await?;

    let date_str = resp
        .headers()
        .get("date")
        .ok_or("no date header")?
        .to_str()?;

    // HTTP date format: "Mon, 16 Feb 2026 01:11:59 GMT"
    // Replace GMT with +0000 so chrono's RFC 2822 parser accepts it
    let dt = chrono::DateTime::parse_from_rfc2822(&date_str.replace("GMT", "+0000"))?;

    let ts = libc::timespec {
        tv_sec: dt.timestamp() as _,
        tv_nsec: 0,
    };
    let ret = unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &ts) };
    if ret != 0 {
        return Err("clock_settime failed".into());
    }

    Ok(())
}
