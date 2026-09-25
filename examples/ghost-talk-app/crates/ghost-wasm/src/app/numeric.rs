pub(super) fn decimal_cmp(left: &str, right: &str) -> i32 {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    match left.len().cmp(&right.len()).then_with(|| left.cmp(right)) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}
