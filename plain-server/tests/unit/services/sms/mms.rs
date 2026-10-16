use super::*;
fn pending() -> Pending {
    Pending {
        id: "pending".into(),
        number: "+12025550123".into(),
        body: "Hello".into(),
        thread_id: "42".into(),
        attachments: vec![Attachment {
            path: "a.jpg".into(),
            content_type: "image/jpeg".into(),
            name: "a.jpg".into(),
        }],
        launch_time_sec: 100,
        minimum_id: 10,
        created_at: String::new(),
    }
}
fn candidate(id: i64, address: &str, body: &str, thread: &str, types: &[&str]) -> Candidate {
    Candidate {
        id,
        address_candidates: vec![crate::services::sms::addresses::MmsAddress {
            value: address.into(),
            is_sender: false,
        }],
        parts: std::iter::once(super::super::parts::Part {
            path: "text".into(),
            content_type: "text/plain".into(),
            name: String::new(),
            file_name: String::new(),
            has_data: false,
            text: body.into(),
        })
        .chain(types.iter().map(|mime| super::super::parts::Part {
            path: "attachment".into(),
            content_type: (*mime).into(),
            name: String::new(),
            file_name: String::new(),
            has_data: true,
            text: String::new(),
        }))
        .collect(),
        thread_id: thread.into(),
    }
}
#[test]
fn provider_matching_requires_recipient_body_thread_and_attachment_families() {
    let candidates = vec![
        candidate(
            11,
            "+12025550123",
            "Hello",
            "42",
            &["image/png; charset=utf-8", "application/smil"],
        ),
        candidate(12, "+12025550124", "Hello", "42", &["image/jpeg"]),
        candidate(13, "+12025550123", "Other", "42", &["image/jpeg"]),
        candidate(14, "+12025550123", "Hello", "43", &["image/jpeg"]),
        candidate(15, "+12025550123", "Hello", "42", &["video/mp4"]),
        candidate(
            16,
            "+12025550123",
            "Hello",
            "42",
            &["image/jpeg", "image/jpeg"],
        ),
    ];
    assert_eq!(matching(&pending(), candidates), vec![11]);
}
#[test]
fn attachment_normalization_preserves_count_and_ignores_smil() {
    assert_eq!(
        normalized_types(
            ["IMAGE/JPEG; charset=utf-8", "application/smil", "image/png"].into_iter()
        ),
        vec!["image/*", "image/*"]
    );
    assert_ne!(
        normalized_types(["image/jpeg"].into_iter()),
        normalized_types(["image/jpeg", "image/png"].into_iter())
    );
}
