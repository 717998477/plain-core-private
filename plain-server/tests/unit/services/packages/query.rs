use super::*;

fn package(id: &str, name: &str, kind: &str, size: i64, time: &str) -> Value {
    json!({"item":{"id":id,"name":name,"type":kind,"size":size,"updatedAt":time,"certs":[{"issuer":"Certificate % issuer","subject":"subject"}]},"nameSortKey":name.to_lowercase()})
}
#[test]
fn packages_filter_certificates_types_and_page_after_sorting() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db.sqlite")).unwrap();
    let facts = vec![
        package("a", "Zeta", "USER", 5, "2026-10-05T00:00:00.000Z"),
        package("b", "Alpha", "SYSTEM", 9, "2026-10-05T02:00:00+02:00"),
        package("c", "Beta", "USER", 7, "2026-10-05T01:00:00.000Z"),
    ];
    let result = packages(&db, facts.clone(), "type:USER text:'% issuer'", "SIZE_DESC").unwrap();
    assert_eq!(
        result.into_iter().skip(1).take(1).collect::<Vec<_>>()[0]["id"],
        "a"
    );
    assert_eq!(
        packages(&db, facts.clone(), "ids:a,c", "DATE_DESC").unwrap()[0]["id"],
        "c"
    );
    assert!(
        packages(&db, facts.clone(), "text:'% nonmatch'", "NAME_ASC")
            .unwrap()
            .is_empty()
    );
    assert_eq!(packages(&db, facts, "", "NAME_DESC").unwrap()[0]["id"], "b");
}
