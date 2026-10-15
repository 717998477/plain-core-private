use super::*;

#[test]
fn shared_header_detection_preserves_animated_and_static_container_rules() {
    let directory = tempfile::tempdir().unwrap();
    assert!(is_animated_image_or_svg(&directory.path().join("logo.svg")));
    assert!(!is_animated_image_or_svg(
        &directory.path().join("photo.png")
    ));
    for (name, bytes, animated) in [
        ("one.gif", b"GIF89a\0\0".to_vec(), true),
        ("two.webp", b"RIFF\0\0\0\0WEBPVP8X\x02\0".to_vec(), true),
        ("three.webp", b"RIFF\0\0\0\0WEBPVP8X\0\0".to_vec(), false),
        ("four.heic", b"\0\0\0\0ftypmsf1".to_vec(), true),
        ("five.heic", b"\0\0\0\0ftypheic".to_vec(), false),
        ("six.txt", b"<svg/>".to_vec(), false),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(is_animated_image_or_svg(&path), animated, "{name}");
    }
}
