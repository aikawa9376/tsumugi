//! Product configuration and instance state stay in Tsumugi's namespace.
use std::path::Path;
use tsumugi_model::paths;

#[test]
fn default_and_named_instances_use_tsumugi_state() {
    assert_eq!(
        paths::instance_dir(Path::new("/state"), "default"),
        Path::new("/state/tsumugi")
    );
    assert_eq!(
        paths::instance_dir(Path::new("/state"), "review"),
        Path::new("/state/tsumugi/instances/review")
    );
}

#[test]
fn settings_use_the_product_namespace() {
    assert!(paths::config_dir().ends_with("tsumugi"));
}
