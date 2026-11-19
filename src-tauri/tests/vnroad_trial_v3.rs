//! Owner trial of the vnroad labeling workflow (huong_dan_thao_tac_labelflow.md) on real images.
//! Not part of the normal suite. Run with VNROAD_TRIAL=<folder> and VNROAD_STAGE=setup|export:
//!   setup : creates <folder>/project (Road Encroachment, 11 v3 classes; Bỏ qua = built-in 255), imports <folder>/frames,
//!           applies the <folder>/prelabel mask folder (Import mask folder, fill unlabeled pixels).
//!   export: exports approved images as "Semantic + region PNGs", train 100 %, into <folder>/exports.
use labelflow_lib::{
    domain::annotation::*,
    export,
    import::masks::{self, MergeMode},
    services::{library, profile, project},
};
use std::{path::PathBuf, sync::atomic::AtomicBool};

#[test]
#[ignore]
fn vnroad_trial_v3() {
    let dir = PathBuf::from(std::env::var("VNROAD_TRIAL").expect("VNROAD_TRIAL"));
    let stage = std::env::var("VNROAD_STAGE").unwrap_or_default();
    let cancel = AtomicBool::new(false);
    let root = dir.join("project");
    if stage == "setup" {
        std::fs::create_dir_all(&root).unwrap();
        let classes: Vec<String> = [
            "mat_duong #804080",
            "via_he_le #f423e8",
            "nha_tuong #464646",
            "ket_cau_co_dinh #ffc000",
            "xe_may_xe_dap #0000e6",
            "o_to #00008e",
            "xe_tai_xe_buyt #000046",
            "xe_tho_so #5a5aff",
            "nguoi #dc143c",
            "dong_vat #ff8c00",
            "khac #00ff00",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut s =
            project::create_with_classes(&root, "vnroad v3 trial", "road-encroachment-v1", Some(&classes))
                .unwrap();
        let report =
            library::import_paths(&mut s, &[dir.join("frames")], false, &cancel, |_| {}).unwrap();
        println!("import: {report:?}");
        let (prepared, inspection) = masks::inspect_folder(
            &s,
            &dir.join("prelabel"),
            profile::SEMANTIC_LAYER,
            &cancel,
            |_| {},
        )
        .unwrap();
        println!(
            "prelabel: matched {} values {:?} warnings {:?}",
            inspection.matched, inspection.values, inspection.warnings
        );
        let mapping: Vec<PixelMapping> = inspection
            .values
            .iter()
            .map(|&v| PixelMapping { source: v, target: v })
            .collect();
        let report = masks::apply_folder(
            &mut s,
            prepared,
            &mapping,
            MergeMode::FillUnlabeled,
            &cancel,
            |_| {},
        )
        .unwrap();
        println!("apply: {report:?}");
        println!("PROJECT={}", root.display());
    } else if stage == "export" {
        let s = project::open(&root).unwrap();
        let out = dir.join("exports");
        std::fs::create_dir_all(&out).unwrap();
        let options = ExportOptions {
            format: "semantic".into(),
            approved_only: true,
            seed: "vnroad".into(),
            train_percent: 100,
            val_percent: 0,
            skip_unsupported: false,
        };
        let report = export::export(&s, &out, &options, &cancel, |_| {}).unwrap();
        println!("EXPORT={}", report.path);
        println!("images {} warnings {:?}", report.images, report.warnings);
    } else {
        panic!("VNROAD_STAGE must be setup or export");
    }
}
