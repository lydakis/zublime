use client::{ZED_URL_SCHEME, ZUBLIME_URL_SCHEME};
use gpui::{AsyncApp, actions};

actions!(
    cli,
    [
        /// Registers the zublime:// and zed:// URL scheme handlers.
        RegisterZedScheme
    ]
);

pub async fn register_zed_scheme(cx: &AsyncApp) -> anyhow::Result<()> {
    let zublime_task = cx.update(|cx| cx.register_url_scheme(ZUBLIME_URL_SCHEME));
    let zed_task = cx.update(|cx| cx.register_url_scheme(ZED_URL_SCHEME));

    zublime_task.await?;
    zed_task.await?;
    Ok(())
}
