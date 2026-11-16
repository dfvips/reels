#[cfg(target_os = "android")]
use super::NATIVE_EDITOR_SCROLL_HANDLE;
use super::{
    ACTIVE_NATIVE_EDITOR_ID, EDIT_STATES, FOCUS_HANDLES, FOCUS_SUBS, INPUT_ENTITIES,
    NATIVE_EDITOR_FOCUSED, NATIVE_EDITOR_VISIBLE, TEXT_ORIGINS,
};
use gpui::ElementId;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[cfg(target_os = "android")]
use gpui::{point, px};

/// Fully discard one input's persistent editor state.
///
/// Unlike [`cleanup_input_state`], this also drops the cached render entity,
/// text origin and active native-editor handoff. Use it when a reusable input
/// ID is reopened with a fresh, saved value and no draft may reappear.
pub fn reset_input_state(id: &ElementId, value: &str) {
    reset_input_state_impl(id, value);
}

/// Reapply a reset after queued focus-out or IME callbacks have drained.
pub fn deferred_input_state_reset(id: &ElementId, value: &str) -> impl FnOnce() + 'static {
    let id = id.clone();
    let value = value.to_string();
    move || reset_input_state_impl(&id, &value)
}

fn reset_input_state_impl(id: &ElementId, value: &str) {
    // Dropping a FocusHandle while its focus-out subscription is alive can
    // commit a stale draft back into the parent. Remove the subscription first.
    FOCUS_SUBS.with(|subs| {
        subs.borrow_mut().remove(id);
    });
    FOCUS_HANDLES.with(|handles| {
        handles.borrow_mut().remove(id);
    });
    EDIT_STATES.with(|states| {
        let mut states = states.borrow_mut();
        states.remove(id);
        states.insert(
            id.clone(),
            Rc::new(RefCell::new(super::EditState::new_fresh(value))),
        );
    });
    TEXT_ORIGINS.with(|origins| {
        origins.borrow_mut().remove(id);
    });
    INPUT_ENTITIES.with(|entities| {
        entities.borrow_mut().remove(id);
    });

    ACTIVE_NATIVE_EDITOR_ID.with(|active| {
        let is_active = active.borrow().as_ref() == Some(id);
        if is_active {
            active.borrow_mut().take();
            NATIVE_EDITOR_VISIBLE.with(|visible: &Cell<bool>| visible.set(false));
            NATIVE_EDITOR_FOCUSED.with(|focused: &Cell<bool>| focused.set(false));
            #[cfg(target_os = "android")]
            NATIVE_EDITOR_SCROLL_HANDLE.with(|handle| {
                if let Some(handle) = handle.borrow_mut().take() {
                    handle.set_offset(point(px(0.0), px(0.0)));
                }
            });
        }
    });
}

/// Clean up thread-local state for an Input element.
///
/// Call this when removing an Input with a dynamic element ID to prevent
/// memory leaks. For static element IDs, cleanup is not necessary.
///
/// # Example
/// ```rust,ignore
/// // When removing a dynamically-created Input
/// cleanup_input_state(&ElementId::Name(format!("input-{}", item_id).into()));
/// ```
pub fn cleanup_input_state(id: &ElementId) {
    FOCUS_SUBS.with(|subs| {
        subs.borrow_mut().remove(id);
    });
    FOCUS_HANDLES.with(|handles| {
        handles.borrow_mut().remove(id);
    });
    EDIT_STATES.with(|states| {
        states.borrow_mut().remove(id);
    });
    TEXT_ORIGINS.with(|origins| {
        origins.borrow_mut().remove(id);
    });
    INPUT_ENTITIES.with(|entities| {
        entities.borrow_mut().remove(id);
    });
}

/// Clean up thread-local state for Input elements whose IDs are not in the retained set.
///
/// This is useful for virtualized lists where you want to keep state only for
/// currently visible items. Call this periodically or when the visible set changes.
///
/// # Example
/// ```rust,ignore
/// // Keep only the inputs currently in view
/// let visible_ids: HashSet<ElementId> = visible_items
///     .iter()
///     .map(|item| ElementId::Name(format!("input-{}", item.id).into()))
///     .collect();
/// cleanup_stale_input_states(&visible_ids);
/// ```
pub fn cleanup_stale_input_states<S: std::hash::BuildHasher>(
    retained_ids: &std::collections::HashSet<ElementId, S>,
) {
    FOCUS_SUBS.with(|subs| {
        subs.borrow_mut().retain(|id, _| retained_ids.contains(id));
    });
    FOCUS_HANDLES.with(|handles| {
        handles
            .borrow_mut()
            .retain(|id, _| retained_ids.contains(id));
    });
    EDIT_STATES.with(|states| {
        states
            .borrow_mut()
            .retain(|id, _| retained_ids.contains(id));
    });
    TEXT_ORIGINS.with(|origins| {
        origins
            .borrow_mut()
            .retain(|id, _| retained_ids.contains(id));
    });
    INPUT_ENTITIES.with(|entities| {
        entities
            .borrow_mut()
            .retain(|id, _| retained_ids.contains(id));
    });
}
