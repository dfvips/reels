//! 输入框所需的精简无障碍桥接（自 ui-kit accessibility 移植）。

use gpui::{Div, Role, SharedString, Stateful, StatefulInteractiveElement};

/// ARIA 角色（本组件仅用到文本框）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AriaRole {
    #[default]
    Textbox,
}

impl AriaRole {
    pub const fn native_role(self) -> Option<Role> {
        match self {
            Self::Textbox => Some(Role::TextInput),
        }
    }
}

/// ARIA 状态（本组件仅用到禁用态）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AriaState {
    Disabled,
}

/// 组件携带的无障碍属性。
#[derive(Debug, Clone, Default)]
pub struct AriaProps {
    pub role: AriaRole,
    pub states: Vec<AriaState>,
    pub value_text: Option<SharedString>,
}

impl AriaProps {
    pub fn with_role(role: AriaRole) -> Self {
        Self {
            role,
            ..Default::default()
        }
    }

    pub fn maybe_state(mut self, condition: bool, state: AriaState) -> Self {
        if condition {
            self.states.push(state);
        }
        self
    }

    pub fn value_text(mut self, text: impl Into<SharedString>) -> Self {
        self.value_text = Some(text.into());
        self
    }
}

/// 将无障碍元数据应用到原生 GPUI 元素。
pub fn apply_native_accessibility(
    mut element: Stateful<Div>,
    label: impl Into<SharedString>,
    props: &AriaProps,
) -> Stateful<Div> {
    if let Some(role) = props.role.native_role() {
        element = element.role(role);
    }
    element = element.aria_label(label);
    for state in &props.states {
        match state {
            AriaState::Disabled => {}
        }
    }
    if let Some(value) = &props.value_text {
        element = element.aria_value(value.clone());
    }
    element
}
