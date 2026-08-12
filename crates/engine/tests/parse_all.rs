use worldsim_engine::events::EventPayload;
use worldsim_engine::storage::Storage;

#[test]
fn parse_all_seed_rows() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest.parent().and_then(|p| p.parent()).unwrap();
    let db = repo.join("data").join("out").join("worldsim.db");
    let storage = Storage::open(&db).unwrap();
    let rows = storage.canonical_events_up_to(
        worldsim_engine::SimDate { year: 9999, month: 12, day: 28 },
    ).unwrap();
    let mut bad = 0;
    for ev in &rows {
        if let Err(e) = serde_json::from_value::<EventPayload>(serde_json::to_value(&ev.payload).unwrap()) {
            bad += 1;
            if bad <= 3 {
                println!("FAIL id={} title={} err={}", ev.id, ev.title, e);
            }
        }
    }
    assert_eq!(bad, 0, "{} bad payloads", bad);
}
