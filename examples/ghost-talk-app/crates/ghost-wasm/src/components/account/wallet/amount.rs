pub(super) fn kas_to_sompi(value: &str) -> Result<String, ()> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') {
        return Err(());
    }
    let mut parts = value.split('.');
    let whole = parts.next().ok_or(())?;
    let fraction = parts.next().unwrap_or("");
    validate_kas_parts(whole, fraction, parts.next().is_some())?;
    let whole_num: u128 = whole.parse().map_err(|_| ())?;
    let fraction_num = padded_fraction(fraction)?;
    whole_num
        .checked_mul(100_000_000)
        .and_then(|base| base.checked_add(fraction_num))
        .map(|sompi| sompi.to_string())
        .ok_or(())
}

fn validate_kas_parts(whole: &str, fraction: &str, extra_part: bool) -> Result<(), ()> {
    if extra_part
        || fraction.len() > 8
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        Err(())
    } else {
        Ok(())
    }
}

fn padded_fraction(fraction: &str) -> Result<u128, ()> {
    let mut value = fraction.to_string();
    while value.len() < 8 {
        value.push('0');
    }
    if value.is_empty() {
        Ok(0)
    } else {
        value.parse().map_err(|_| ())
    }
}

pub(super) fn format_kas(sompi: &str) -> String {
    let Ok(value) = sompi.parse::<u128>() else {
        return sompi.into();
    };
    let whole = value / 100_000_000;
    let fraction = value % 100_000_000;
    if fraction == 0 {
        return whole.to_string();
    }
    let mut text = format!("{whole}.{fraction:08}");
    while text.ends_with('0') {
        text.pop();
    }
    text
}
