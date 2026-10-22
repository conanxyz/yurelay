//! Replace a shortcut set without disabling working bindings on conflict.
pub fn replace<T: Copy + PartialEq>(
    old: &[T],
    new: &[T],
    mut register: impl FnMut(T) -> Result<(), String>,
    mut unregister: impl FnMut(T) -> Result<(), String>,
) -> Result<(), String> {
    let mut added = Vec::new();
    for item in new.iter().filter(|item| !old.contains(item)) {
        if let Err(error) = register(*item) {
            for added in added {
                let _ = unregister(added);
            }
            return Err(error);
        }
        added.push(*item);
    }
    let mut removed = Vec::new();
    for item in old.iter().filter(|item| !new.contains(item)) {
        if let Err(error) = unregister(*item) {
            for removed in removed {
                let _ = register(removed);
            }
            for added in added {
                let _ = unregister(added);
            }
            return Err(error);
        }
        removed.push(*item);
    }
    Ok(())
}

pub fn permission_required(message: &str) -> Option<&'static str> {
    match message {
        "请先在设置中开启 Yurelay 的屏幕录制权限。" => Some("screen"),
        "请先在设置中开启 Yurelay 的辅助功能权限。" => Some("accessibility"),
        _ => None,
    }
}

pub fn notice_is_relevant(message: &str, accessibility: bool, screen: bool) -> bool {
    match permission_required(message) {
        Some("screen") => !screen,
        Some("accessibility") => !accessibility,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn second_binding_conflict_preserves_both_old_bindings() {
        let active = RefCell::new(vec![1, 2]);
        let result = replace(
            &[1, 2],
            &[3, 4],
            |key| {
                if key == 4 {
                    return Err("occupied".into());
                }
                active.borrow_mut().push(key);
                Ok(())
            },
            |key| {
                active.borrow_mut().retain(|v| *v != key);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(*active.borrow(), vec![1, 2]);
    }
    #[test]
    fn swapping_actions_does_not_unregister_their_shared_keys() {
        replace(
            &[1, 2],
            &[2, 1],
            |_| panic!("already registered"),
            |_| panic!("still needed"),
        )
        .unwrap();
    }
    #[test]
    fn only_the_missing_permission_keeps_its_notice() {
        let screen = "请先在设置中开启 Yurelay 的屏幕录制权限。";
        assert!(!notice_is_relevant(screen, false, true));
        assert!(notice_is_relevant(screen, true, false));
        assert!(notice_is_relevant("connection failed", true, true));
    }
}

#[cfg(test)]
mod append_tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn fourth_optional_shortcut_conflict_rolls_back_all_new_bindings() {
        let active = RefCell::new(vec![1, 2]);
        let result = replace(
            &[1, 2],
            &[1, 2, 3, 4],
            |key| {
                if key == 4 {
                    return Err("occupied".into());
                }
                active.borrow_mut().push(key);
                Ok(())
            },
            |key| {
                active.borrow_mut().retain(|value| *value != key);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(*active.borrow(), vec![1, 2]);
    }
    #[test]
    fn disabling_optional_bindings_preserves_normal_capture() {
        let active = RefCell::new(vec![1, 2, 3, 4]);
        replace(
            &[1, 2, 3, 4],
            &[1, 2],
            |_| panic!("already registered"),
            |key| {
                active.borrow_mut().retain(|value| *value != key);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*active.borrow(), vec![1, 2]);
    }
}
