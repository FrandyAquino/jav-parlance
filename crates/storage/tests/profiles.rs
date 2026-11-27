#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Barrier};

use parlance_protocol::SaveProjectProfileRequest;
use parlance_storage::{JsonProfileRepository, ProfileRepository};

fn create(name: &str) -> SaveProjectProfileRequest {
    SaveProjectProfileRequest { id: None, name: name.into(), content: "Redis caches reads".into() }
}

#[test]
fn profiles_survive_restart_without_creating_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    let repo = JsonProfileRepository::open(path.clone()).unwrap();
    let first = repo.save(create("Backend")).unwrap().profiles.remove(0);
    repo.set_active(Some(&first.id)).unwrap();
    repo.save(SaveProjectProfileRequest { id: Some(first.id.clone()), name: "API".into(), content: "Redis".into() })
        .unwrap();
    let loaded = JsonProfileRepository::open(path).unwrap().list().unwrap();
    assert_eq!(loaded.active_profile_id, Some(first.id.clone()));
    assert_eq!(loaded.profiles[0].revision, 2);
    assert_eq!(loaded.profiles[0].name, "API");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn concurrent_mutations_preserve_both_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Arc::new(JsonProfileRepository::open(dir.path().join("profiles.json")).unwrap());
    let barrier = Arc::new(Barrier::new(3));
    let handles: Vec<_> = ["A", "B"]
        .into_iter()
        .map(|name| {
            let repo = Arc::clone(&repo);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                repo.save(create(name)).unwrap();
            })
        })
        .collect();
    barrier.wait();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(repo.list().unwrap().profiles.len(), 2);
}

#[test]
fn failed_save_keeps_memory_and_prior_file() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("config");
    let path = folder.join("profiles.json");
    let repo = JsonProfileRepository::open(path.clone()).unwrap();
    repo.save(create("A")).unwrap();
    let before = std::fs::read(&path).unwrap();
    let preserved = dir.path().join("preserved");
    std::fs::rename(&folder, &preserved).unwrap();
    std::fs::write(&folder, "not a directory").unwrap();
    assert!(repo.save(create("B")).is_err());
    assert_eq!(repo.list().unwrap().profiles.len(), 1);
    assert_eq!(std::fs::read(preserved.join("profiles.json")).unwrap(), before);
}

#[test]
fn deleting_active_profile_resets_selection_and_snapshots_remain_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    let repo = JsonProfileRepository::open(path.clone()).unwrap();
    let a = repo.save(create("A")).unwrap().profiles.remove(0);
    let b = repo.save(create("B")).unwrap().profiles.remove(1);
    repo.set_active(Some(&a.id)).unwrap();
    let snapshot = repo.active_snapshot().unwrap();
    repo.save(SaveProjectProfileRequest {
        id: Some(a.id.clone()),
        name: "Updated A".into(),
        content: "Different facts".into(),
    })
    .unwrap();
    assert_eq!(repo.active_snapshot().unwrap().revision, 2);
    repo.delete(&b.id).unwrap();
    assert_eq!(repo.list().unwrap().active_profile_id.as_deref(), Some(a.id.as_str()));
    repo.delete(&a.id).unwrap();
    assert!(JsonProfileRepository::open(path).unwrap().list().unwrap().active_profile_id.is_none());
    assert_eq!(snapshot.name, "A");
    assert_eq!(snapshot.content, "Redis caches reads");
    assert_eq!(snapshot.revision, 1);
    assert!(repo.snapshot(Some(&a.id)).is_err());
    assert!(repo.snapshot(None).unwrap().is_none());
}

#[test]
fn invalid_or_unsupported_files_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.json");
    for bytes in [b"{broken".as_slice(), br#"{"version":999,"profiles":[],"activeProfileId":null,"nextId":1}"#] {
        std::fs::write(&path, bytes).unwrap();
        assert!(JsonProfileRepository::open(path.clone()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn validation_rejects_unknown_ids_empty_names_and_oversized_content() {
    let dir = tempfile::tempdir().unwrap();
    let repo = JsonProfileRepository::open(dir.path().join("profiles.json")).unwrap();
    assert!(repo.save(create("  ")).is_err());
    assert!(repo.save(create(&"a".repeat(81))).is_err());
    assert!(repo.save(SaveProjectProfileRequest { content: "é".repeat(6001), ..create("A") }).is_err());
    assert!(repo.save(SaveProjectProfileRequest { id: Some("missing".into()), ..create("A") }).is_err());
    assert!(repo.set_active(Some("missing")).is_err());
    assert!(repo.delete("missing").is_err());
    assert!(repo.list().unwrap().profiles.is_empty());
}
