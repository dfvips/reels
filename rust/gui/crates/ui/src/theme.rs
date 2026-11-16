//! 颜色与排版常量。

use gpui::{rgb, rgba, Rgba};

pub const PRIMARY: u32 = 0x3f8dff;
pub const ACCENT: u32 = 0x3f8dff;
pub const PRIMARY_GRADIENT_START: u32 = 0x0277f9;
pub const PRIMARY_GRADIENT_END: u32 = 0x5d47e3;

#[derive(Clone, Copy)]
pub struct ButtonColors {
    pub bg: Rgba,
    pub border: Rgba,
    pub fg: Rgba,
    pub hover_bg: Rgba,
    pub gradient: bool,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub primary: Rgba,
    pub bg: Rgba,
    pub text: Rgba,
    pub text_secondary: Rgba,
    pub text_muted: Rgba,
    pub divider: Rgba,
    pub input_border: Rgba,
    pub input_focus: Rgba,
    pub hint: Rgba,
    pub result_bg: Rgba,
    pub path_bg: Rgba,
    pub result_item_bg: Rgba,
    pub window_btn_hover: Rgba,
    pub window_icon: Rgba,
    pub close_hover: Rgba,
    pub toast_bg: Rgba,
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                primary: rgb(PRIMARY),
                bg: rgb(0x141414),
                text: rgb(0xffffff),
                text_secondary: rgba(0xffffffb3),
                text_muted: rgba(0xffffff80),
                divider: rgb(0x333333),
                input_border: rgb(0x4c4d4f),
                input_focus: rgb(PRIMARY),
                hint: rgb(0xc0c4cc),
                result_bg: rgb(0x1a222c),
                path_bg: rgb(0x3c3c3c),
                result_item_bg: rgba(0x0000004d),
                window_btn_hover: rgb(0x2d2d2d),
                window_icon: rgb(0xffffff),
                close_hover: rgb(0xc42c1b),
                toast_bg: rgb(0x141414),
            }
        } else {
            Self {
                dark,
                primary: rgb(PRIMARY),
                bg: rgb(0xffffff),
                text: rgb(0x000000),
                text_secondary: rgb(0x606266),
                text_muted: rgb(0x909399),
                divider: rgb(0xebeef5),
                input_border: rgb(0xcac4d0),
                input_focus: rgb(PRIMARY),
                hint: rgb(0xc0c4cc),
                result_bg: rgb(0xeef5ff),
                path_bg: rgb(0xf8f8fc),
                result_item_bg: rgb(0xffffff),
                window_btn_hover: rgb(0xe5e5e5),
                window_icon: rgb(0x1a1a1a),
                close_hover: rgb(0xc42c1b),
                toast_bg: rgb(0xffffff),
            }
        }
    }

    pub fn paste_button(&self) -> ButtonColors {
        if self.dark {
            ButtonColors {
                bg: rgb(0x141414),
                border: rgb(0x4c4d4f),
                fg: rgb(0xffffff),
                hover_bg: rgb(0x1f1f1f),
                gradient: false,
            }
        } else {
            ButtonColors {
                bg: rgb(0xffffff),
                border: rgb(ACCENT),
                fg: rgb(ACCENT),
                hover_bg: rgba(0x409eff33),
                gradient: false,
            }
        }
    }

    pub fn clear_button(&self, enabled: bool) -> ButtonColors {
        if !enabled {
            return if self.dark {
                ButtonColors {
                    bg: rgb(0x141414),
                    border: rgb(0x4c4d4f),
                    fg: rgb(0x4c4d4f),
                    hover_bg: rgb(0x141414),
                    gradient: false,
                }
            } else {
                ButtonColors {
                    bg: rgb(0xffffff),
                    border: rgb(0xebeef5),
                    fg: rgb(0xc0c4cc),
                    hover_bg: rgb(0xffffff),
                    gradient: false,
                }
            };
        }
        if self.dark {
            ButtonColors {
                bg: rgb(0x141414),
                border: rgb(ACCENT),
                fg: rgb(ACCENT),
                hover_bg: rgba(0x3f8dff33),
                gradient: false,
            }
        } else {
            ButtonColors {
                bg: rgb(0xffffff),
                border: rgb(ACCENT),
                fg: rgb(ACCENT),
                hover_bg: rgba(0x3f8dff14),
                gradient: false,
            }
        }
    }

    pub fn parse_button(&self, enabled: bool) -> ButtonColors {
        if enabled {
            ButtonColors {
                bg: self.primary,
                border: self.primary,
                fg: rgb(0xffffff),
                hover_bg: rgb(0x69a0ff),
                gradient: true,
            }
        } else if self.dark {
            ButtonColors {
                bg: rgb(0x141414),
                border: rgb(0x4c4d4f),
                fg: rgb(0x4c4d4f),
                hover_bg: rgb(0x141414),
                gradient: false,
            }
        } else {
            ButtonColors {
                bg: rgb(0xffffff),
                border: rgb(0xebeef5),
                fg: rgb(0xc0c4cc),
                hover_bg: rgb(0xffffff),
                gradient: false,
            }
        }
    }

    pub fn copy_button(&self) -> ButtonColors {
        if self.dark {
            ButtonColors {
                bg: rgb(0x141414),
                border: rgb(0x4c4d4f),
                fg: rgb(0xffffff),
                hover_bg: rgb(0x1f1f1f),
                gradient: false,
            }
        } else {
            ButtonColors {
                bg: rgb(0xffffff),
                border: rgb(ACCENT),
                fg: rgb(ACCENT),
                hover_bg: rgba(0x409eff33),
                gradient: false,
            }
        }
    }

    pub fn export_button(&self) -> ButtonColors {
        if self.dark {
            ButtonColors {
                bg: rgb(0x18232b),
                border: rgb(ACCENT),
                fg: rgb(ACCENT),
                hover_bg: rgb(0x1e2d38),
                gradient: false,
            }
        } else {
            ButtonColors {
                bg: rgb(0xecf5ff),
                border: rgb(0xb3d8ff),
                fg: rgb(ACCENT),
                hover_bg: rgb(0xdcecff),
                gradient: false,
            }
        }
    }

    pub fn download_button(&self) -> ButtonColors {
        ButtonColors {
            bg: self.primary,
            border: self.primary,
            fg: rgb(0xffffff),
            hover_bg: rgb(0x69a0ff),
            gradient: true,
        }
    }

    pub fn plain_button(&self) -> ButtonColors {
        if self.dark {
            ButtonColors {
                bg: rgba(0xffffff14),
                border: rgb(0x4c4d4f),
                fg: self.text_secondary,
                hover_bg: rgba(0xffffff08),
                gradient: false,
            }
        } else {
            ButtonColors {
                bg: rgb(0xffffff),
                border: rgb(0xdcdfe6),
                fg: self.text_secondary,
                hover_bg: rgba(0x0000000f),
                gradient: false,
            }
        }
    }

    pub fn outline_button(&self) -> ButtonColors {
        ButtonColors {
            bg: rgba(0x00000000),
            border: rgb(ACCENT),
            fg: rgb(ACCENT),
            hover_bg: rgba(0x3f8dff14),
            gradient: false,
        }
    }
}

pub fn title_font_size() -> f32 {
    20.0
}

pub fn mono_font() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Menlo"
    }
    #[cfg(target_os = "windows")]
    {
        "Consolas"
    }
    #[cfg(target_os = "android")]
    {
        "Droid Sans Mono"
    }
    #[cfg(target_os = "ios")]
    {
        // GPUI's generic "monospace" alias is not resolvable on iOS and its
        // missing-font fallback path panics. UIKit's system font is always
        // present and keeps help/documentation dialogs renderable.
        ".SystemUIFont"
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "android",
        target_os = "ios"
    )))]
    {
        "monospace"
    }
}

pub fn ui_font() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "PingFang SC"
    }
    #[cfg(target_os = "windows")]
    {
        "Microsoft YaHei UI"
    }
    #[cfg(target_os = "android")]
    {
        ".SystemUIFont"
    }
    #[cfg(target_os = "ios")]
    {
        ".SystemUIFont"
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "android",
        target_os = "ios"
    )))]
    {
        "sans-serif"
    }
}
