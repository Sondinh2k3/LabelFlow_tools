//! Exports a COPY of an owner project with the real LabelFlow export code, to inspect the
//! output format (CLAUDE_CODE_TASK_LabelFlow.md, task 2). Not part of the normal suite.
//!   VNROAD_PROJECT=<copied project root (folder that contains .labelflow)>
//!   VNROAD_OUT=<export parent folder>
//!   VNROAD_APPROVED_ONLY=0|1 (default 1), VNROAD_TRAIN=<0-100> (default 100), VNROAD_VAL (default 0)
//! Never point VNROAD_PROJECT at the live project: export writes dataset_splits.
use labelflow_lib::{domain::annotation::*, export, services::project};
use std::{path::PathBuf, sync::atomic::AtomicBool};

#[test]
#[ignore]
fn vnroad_export_copy() {
    let root = PathBuf::from(std::env::var("VNROAD_PROJECT").expect("VNROAD_PROJECT"));
    let out = PathBuf::from(std::env::var("VNROAD_OUT").expect("VNROAD_OUT"));
    let num = |k: &str, d: u8| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let s = project::open(&root).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let options = ExportOptions {
        format: "semantic".into(),
        approved_only: std::env::var("VNROAD_APPROVED_ONLY").map_or(true, |v| v != "0"),
        seed: "vnroad".into(),
        train_percent: num("VNROAD_TRAIN", 100),
        val_percent: num("VNROAD_VAL", 0),
        skip_unsupported: false,
    };
    let report = export::export(&s, &out, &options, &AtomicBool::new(false), |_| {}).unwrap();
    println!("EXPORT={}", report.path);
    println!("images {} warnings {:?}", report.images, report.warnings);
}
