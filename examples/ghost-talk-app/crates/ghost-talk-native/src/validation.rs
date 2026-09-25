#![forbid(unsafe_code)]

pub(crate) fn require_min_password(password: &str, label: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err(format!("{label} password must be at least 8 characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::require_min_password;

    #[test]
    fn minimum_password_policy_is_shared_across_native_subsystems() {
        assert!(require_min_password("1234567", "ID").is_err());
        assert!(require_min_password("12345678", "ID").is_ok());
        assert_eq!(
            require_min_password("short", "wallet").unwrap_err(),
            "wallet password must be at least 8 characters"
        );
    }
}
