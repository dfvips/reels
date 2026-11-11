//! 输入框主题与外观变体（自 ui-kit 移植的精简版）。

use gpui::Rgba;
use gpui_ui_kit::theme::Theme;

/// 输入框配色（等价于 ui-kit `InputTheme` 的 `ComponentTheme` 展开结果）。
#[derive(Debug, Clone, Copy)]
pub struct InputTheme {
    pub background: Rgba,
    pub filled_bg: Rgba,
    pub text: Rgba,
    pub placeholder: Rgba,
    pub label: Rgba,
    pub border: Rgba,
    pub border_hover: Rgba,
    pub border_focus: Rgba,
    pub error: Rgba,
    pub cursor: Rgba,
    pub selection_bg: Rgba,
    pub transparent: Rgba,
}

impl InputTheme {
    /// 从全局 ui-kit 主题映射（与 ui-kit `#[theme(from = ...)]` 语义一致）。
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            background: theme.background,
            filled_bg: theme.surface,
            text: theme.text_primary,
            placeholder: theme.text_muted,
            label: theme.text_secondary,
            border: theme.border,
            border_hover: theme.border_hover,
            border_focus: theme.accent,
            error: theme.error,
            cursor: theme.accent,
            selection_bg: theme.accent.alpha(0.3),
            transparent: theme.transparent,
        }
    }
}

/// 输入框视觉变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputVariant {
    #[default]
    Default,
    Filled,
    Flushed,
}
