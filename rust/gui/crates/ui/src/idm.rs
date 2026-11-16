use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct IdmLink {
    pub url: String,
    pub referer: String,
    pub cookie: Option<String>,
    pub file_name: String,
}

pub type IdmExportHook = dyn Fn(&[IdmLink]) -> anyhow::Result<()> + Send + Sync;

static HOOK: OnceLock<Box<IdmExportHook>> = OnceLock::new();

pub fn set_idm_export_hook(hook: Option<Box<IdmExportHook>>) {
    if let Some(hook) = hook {
        let _ = HOOK.set(hook);
    }
}

pub(crate) fn export_links(links: &[IdmLink]) -> anyhow::Result<()> {
    match HOOK.get() {
        Some(hook) => hook(links),
        None => anyhow::bail!("IDM 导出功能未初始化"),
    }
}
