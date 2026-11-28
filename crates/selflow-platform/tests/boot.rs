use selflow_platform::boot::boot_id;

#[test]
fn the_boot_id_is_stable_within_a_boot() {
    let first = boot_id().unwrap();
    assert!(!first.is_empty());
    assert_eq!(first, boot_id().unwrap());
}
