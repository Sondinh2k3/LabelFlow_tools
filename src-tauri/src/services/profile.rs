use crate::domain::{
    annotation::{ClassDefinition, LayerDefinition, Profile},
    Result,
};
use crate::services::{
    annotation::{decode_png, invalid},
    project::ProjectSession,
};
use rusqlite::{params, Connection, OptionalExtension};

/// Object classes (bbox/polygon/instance) and semantic classes are project data: users may add,
/// rename, recolor and (when unused) delete them. Layers stay fixed by the built-in profile.
pub const OBJECT_NAMESPACE: &str = "candidate";
/// Historic identifiers kept for compatibility with existing projects and export paths; the
/// semantic class set itself is project-defined (Cityscapes train IDs are only the default).
pub const SEMANTIC_NAMESPACE: &str = "cityscapes";
pub const SEMANTIC_LAYER: &str = "cityscapes_semantic";
const MAX_CLASSES: usize = 255;
const PALETTE: [&str; 12] = [
    "#804080", "#ffd43b", "#f423e8", "#0000e6", "#dc143c", "#a9e34b", "#ff922b", "#464646",
    "#38d9a9", "#4dabf7", "#da77f2", "#f783ac",
];

pub fn definition(id: &str) -> Profile {
    let road = id == "road-encroachment-v1";
    let names = [
        "road",
        "sidewalk",
        "building",
        "wall",
        "fence",
        "pole",
        "traffic light",
        "traffic sign",
        "vegetation",
        "terrain",
        "sky",
        "person",
        "rider",
        "car",
        "truck",
        "bus",
        "train",
        "motorcycle",
        "bicycle",
    ];
    let colors = [
        "#804080", "#f423e8", "#464646", "#66669c", "#be9999", "#999999", "#faaa1e", "#dcdc00",
        "#6b8e23", "#98fb98", "#4682b4", "#dc143c", "#ff0000", "#00008e", "#000046", "#003c64",
        "#005064", "#0000e6", "#770b20",
    ];
    let mut classes: Vec<ClassDefinition> = names
        .iter()
        .enumerate()
        .map(|(i, name)| ClassDefinition {
            id: format!("cityscapes-{i}"),
            namespace: "cityscapes".into(),
            stable_key: name.replace(' ', "_"),
            export_id: i as u8,
            display_name: name.to_string(),
            color: colors[i].into(),
            is_ignore: false,
        })
        .collect();
    classes.push(ClassDefinition {
        id: "cityscapes-ignore".into(),
        namespace: "cityscapes".into(),
        stable_key: "ignore".into(),
        export_id: 255,
        display_name: "Ignore / uncertain".into(),
        color: "#808080".into(),
        is_ignore: true,
    });
    let objects: &[&str] = if road {
        &[
            "material_pile",
            "goods_cluster",
            "street_stall",
            "cart",
            "standalone_object",
        ]
    } else {
        &["object"]
    };
    classes.extend(objects.iter().enumerate().map(|(i, name)| ClassDefinition {
        id: format!("candidate-{i}"),
        namespace: "candidate".into(),
        stable_key: name.to_string(),
        export_id: i as u8,
        display_name: name.replace('_', " "),
        color: ["#ffb86b", "#a2e8bd", "#be99ff", "#72cfff", "#ff85a1"][i].into(),
        is_ignore: false,
    }));
    let mut layers = vec![
        LayerDefinition {
            id: "candidate_objects".into(),
            stable_key: "candidate_objects".into(),
            layer_type: "instance".into(),
            display_name: "Candidate objects".into(),
            namespace: Some("candidate".into()),
            color: "#ffb86b".into(),
        },
        LayerDefinition {
            id: "cityscapes_semantic".into(),
            stable_key: "cityscapes_semantic".into(),
            layer_type: "semantic".into(),
            display_name: "Cityscapes semantic".into(),
            namespace: Some("cityscapes".into()),
            color: "#804080".into(),
        },
    ];
    if road {
        for (key, name, color) in [
            ("road_extent", "Road Extent", "#52d8ff"),
            ("traffic_exclusion", "Traffic exclusion", "#ffc857"),
            ("suspected_region", "Suspected region", "#ff6b88"),
        ] {
            layers.push(LayerDefinition {
                id: key.into(),
                stable_key: key.into(),
                layer_type: "region".into(),
                display_name: name.into(),
                namespace: None,
                color: color.into(),
            });
        }
    }
    Profile {
        id: id.into(),
        classes,
        layers,
    }
}
pub fn ensure(connection: &mut Connection, project_id: &str, profile_id: &str) -> Result<Profile> {
    ensure_with(connection, project_id, profile_id, None)
}
/// Seeds classes once per namespace; deletions are never resurrected on reopen. `semantic`
/// replaces the default Cityscapes set at project creation (IDs follow list order).
pub fn ensure_with(
    connection: &mut Connection,
    project_id: &str,
    profile_id: &str,
    semantic: Option<&[String]>,
) -> Result<Profile> {
    let mut profile = definition(profile_id);
    if let Some(lines) = semantic {
        let custom = custom_semantic(lines)?;
        profile
            .classes
            .retain(|c| c.namespace != SEMANTIC_NAMESPACE || c.is_ignore);
        profile.classes.splice(0..0, custom);
    }
    let tx = connection.transaction()?;
    let seeded = |ns: &str| -> Result<bool> {
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM classes WHERE project_id=?1 AND namespace=?2 AND is_ignore=0)",
            params![project_id, ns],
            |r| r.get(0),
        )?)
    };
    let (has_objects, has_semantic) = (seeded(OBJECT_NAMESPACE)?, seeded(SEMANTIC_NAMESPACE)?);
    for (i, c) in profile.classes.iter().enumerate() {
        if !c.is_ignore
            && ((has_objects && c.namespace == OBJECT_NAMESPACE)
                || (has_semantic && c.namespace == SEMANTIC_NAMESPACE))
        {
            continue;
        }
        tx.execute("INSERT OR IGNORE INTO classes(id,project_id,namespace,stable_key,export_id,display_name,color,is_ignore,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![c.id,project_id,c.namespace,c.stable_key,c.export_id,c.display_name,c.color,c.is_ignore,i])?;
    }
    for l in &profile.layers {
        tx.execute("INSERT OR IGNORE INTO layers(id,project_id,stable_key,layer_type,display_name,namespace,color,export_role) VALUES(?1,?2,?3,?4,?5,?6,?7,?3)",params![l.id,project_id,l.stable_key,l.layer_type,l.display_name,l.namespace,l.color])?;
    }
    tx.commit()?;
    load(connection, project_id, profile_id)
}
/// Parses "Name" or "Name #rrggbb" lines; blank lines are skipped. Export IDs are 0..n-1.
pub fn custom_semantic(lines: &[String]) -> Result<Vec<ClassDefinition>> {
    let mut classes: Vec<ClassDefinition> = Vec::new();
    for line in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let (name, color) = match line.rsplit_once(' ') {
            Some((name, color)) if valid_color(color) => (name, color.to_lowercase()),
            _ => (line, PALETTE[classes.len() % PALETTE.len()].to_string()),
        };
        let name = clean_name(name)?;
        if classes
            .iter()
            .any(|c| c.display_name.to_lowercase() == name.to_lowercase())
        {
            return Err(invalid(format!(
                "Semantic class \u{201c}{name}\u{201d} is listed twice."
            )));
        }
        if classes.len() >= MAX_CLASSES {
            return Err(invalid(
                "Semantic layers support at most 255 classes (IDs 0-254).",
            ));
        }
        let base = stable_key(&name);
        let mut key = base.clone();
        let mut n = 2;
        while classes.iter().any(|c| c.stable_key == key) || key == "ignore" {
            key = format!("{base}_{n}");
            n += 1;
        }
        classes.push(ClassDefinition {
            id: format!("semantic-{}", uuid::Uuid::new_v4()),
            namespace: SEMANTIC_NAMESPACE.into(),
            stable_key: key,
            export_id: classes.len() as u8,
            display_name: name,
            color,
            is_ignore: false,
        });
    }
    if classes.is_empty() {
        return Err(invalid("List at least one semantic class, one per line."));
    }
    Ok(classes)
}

/// Profile of the open project, including user-defined classes stored in SQLite.
pub fn current(session: &ProjectSession) -> Result<Profile> {
    load(
        &session.connection,
        &session.project.id,
        &session.project.profile_id,
    )
}
fn stored(
    connection: &Connection,
    project_id: &str,
    namespace: &str,
) -> Result<Vec<ClassDefinition>> {
    // Semantic classes list by ID (ignore last), which matches the historic Cityscapes seed order.
    let order = if namespace == SEMANTIC_NAMESPACE {
        "is_ignore,export_id"
    } else {
        "sort_order,export_id,id"
    };
    let mut statement = connection.prepare(&format!(
        "SELECT id,stable_key,export_id,display_name,color,is_ignore FROM classes WHERE project_id=?1 AND namespace=?2 ORDER BY {order}"
    ))?;
    let rows = statement
        .query_map(params![project_id, namespace], |r| {
            Ok(ClassDefinition {
                id: r.get(0)?,
                namespace: namespace.into(),
                stable_key: r.get(1)?,
                export_id: r.get(2)?,
                display_name: r.get(3)?,
                color: r.get(4)?,
                is_ignore: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
pub fn load(connection: &Connection, project_id: &str, profile_id: &str) -> Result<Profile> {
    let mut profile = definition(profile_id);
    let mut classes = Vec::with_capacity(profile.classes.len());
    for namespace in [SEMANTIC_NAMESPACE, OBJECT_NAMESPACE] {
        let builtin = profile.classes.iter().filter(|c| c.namespace == namespace);
        let rows = stored(connection, project_id, namespace)?;
        if !rows.iter().any(|c| !c.is_ignore) {
            classes.extend(builtin.cloned());
            continue;
        }
        let has_ignore = rows.iter().any(|c| c.is_ignore);
        classes.extend(rows);
        if !has_ignore {
            classes.extend(builtin.filter(|c| c.is_ignore).cloned());
        }
    }
    profile.classes = classes;
    Ok(profile)
}
fn valid_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(invalid(
            "Class names need 1–64 characters without control characters.",
        ));
    }
    Ok(name.to_owned())
}
fn stable_key(name: &str) -> String {
    let key = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let key = key.trim_matches('_');
    if key.is_empty() {
        "class".into()
    } else {
        key.chars().take(48).collect()
    }
}
/// Creates (id = None) or renames/recolors a class. Export IDs are never renumbered; a new
/// class takes the smallest free ID. `namespace` selects object (default) or semantic classes.
pub fn save_class(
    session: &mut ProjectSession,
    namespace: Option<&str>,
    id: Option<&str>,
    name: &str,
    color: &str,
) -> Result<Profile> {
    let name = clean_name(name)?;
    if !valid_color(color) {
        return Err(invalid("Class color must look like #12abef."));
    }
    let project_id = session.project.id.clone();
    let namespace = match id {
        Some(id) => editable(session, id)?.0,
        None => match namespace.unwrap_or(OBJECT_NAMESPACE) {
            OBJECT_NAMESPACE => OBJECT_NAMESPACE.to_string(),
            SEMANTIC_NAMESPACE => SEMANTIC_NAMESPACE.to_string(),
            _ => return Err(invalid("Unknown class namespace.")),
        },
    };
    let profile = current(session)?;
    let classes = profile
        .classes
        .iter()
        .filter(|c| c.namespace == namespace)
        .collect::<Vec<_>>();
    if classes
        .iter()
        .any(|c| Some(c.id.as_str()) != id && c.display_name.to_lowercase() == name.to_lowercase())
    {
        return Err(invalid(
            "Another class in this layer already uses this name.",
        ));
    }
    let tx = session.connection.transaction()?;
    match id {
        Some(id) => {
            let changed = tx.execute(
                "UPDATE classes SET display_name=?1,color=?2 WHERE id=?3 AND project_id=?4 AND namespace=?5 AND is_ignore=0",
                params![name, color, id, project_id, namespace],
            )?;
            if changed != 1 {
                return Err(invalid("Unknown class."));
            }
        }
        None => {
            let used: Vec<u8> = classes
                .iter()
                .filter(|c| !c.is_ignore)
                .map(|c| c.export_id)
                .collect();
            if used.len() >= MAX_CLASSES {
                return Err(invalid("A layer supports at most 255 classes."));
            }
            let export_id = (0u8..=254)
                .find(|v| !used.contains(v))
                .ok_or_else(|| invalid("No free export ID is left."))?;
            let base = stable_key(&name);
            let mut key = base.clone();
            let mut n = 2;
            while tx
                .query_row(
                    "SELECT 1 FROM classes WHERE project_id=?1 AND namespace=?2 AND stable_key=?3",
                    params![project_id, namespace, key],
                    |_| Ok(()),
                )
                .optional()?
                .is_some()
            {
                key = format!("{base}_{n}");
                n += 1;
            }
            let order: i64 = tx.query_row(
                "SELECT coalesce(max(sort_order),0)+1 FROM classes WHERE project_id=?1",
                [&project_id],
                |r| r.get(0),
            )?;
            let prefix = if namespace == SEMANTIC_NAMESPACE {
                "semantic"
            } else {
                "custom"
            };
            tx.execute(
                "INSERT INTO classes(id,project_id,namespace,stable_key,export_id,display_name,color,is_ignore,sort_order) VALUES(?1,?2,?3,?4,?5,?6,?7,0,?8)",
                params![
                    format!("{prefix}-{}", uuid::Uuid::new_v4()),
                    project_id,
                    namespace,
                    key,
                    export_id,
                    name,
                    color,
                    order
                ],
            )?;
        }
    }
    tx.commit()?;
    current(session)
}
/// (namespace, export ID) of a user-editable class; the semantic ignore class (255) is fixed.
fn editable(session: &ProjectSession, id: &str) -> Result<(String, u8)> {
    let row: Option<(String, u8, bool)> = session
        .connection
        .query_row(
            "SELECT namespace,export_id,is_ignore FROM classes WHERE id=?1 AND project_id=?2",
            params![id, session.project.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    match row {
        Some((ns, export_id, false)) if ns == OBJECT_NAMESPACE || ns == SEMANTIC_NAMESPACE => {
            Ok((ns, export_id))
        }
        Some((_, _, true)) => Err(invalid("The ignore class (255) is fixed.")),
        _ => Err(invalid("Unknown class.")),
    }
}
/// True when any current semantic mask contains `value`. Only the rare delete action pays for
/// decoding the stored PNG assets, and it stops at the first hit.
fn semantic_value_used(session: &ProjectSession, value: u8) -> Result<bool> {
    let mut statement = session.connection.prepare(
        "SELECT DISTINCT m.relative_path FROM annotations a JOIN mask_assets m ON m.id=a.mask_asset_id WHERE a.layer_id=?1 AND a.geometry_type='semantic_layer'",
    )?;
    let paths = statement
        .query_map([SEMANTIC_LAYER], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let base = session.root.join(".labelflow");
    for relative in paths {
        if !relative.starts_with("masks/") || relative.contains("..") {
            return Err(invalid("Unsafe mask asset path."));
        }
        let path = base.join(&relative);
        crate::security::reject_symlink(&path)?;
        let (_, _, pixels) = decode_png(&std::fs::read(path)?)?;
        if pixels.contains(&value) {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Deletes a class only when no saved label uses it. Renaming is always safe.
pub fn delete_class(session: &mut ProjectSession, id: &str) -> Result<Profile> {
    let project_id = session.project.id.clone();
    let (namespace, export_id) = editable(session, id)?;
    let remaining = current(session)?
        .classes
        .into_iter()
        .filter(|c| c.namespace == namespace && !c.is_ignore)
        .count();
    if remaining <= 1 {
        return Err(invalid("Keep at least one class in this layer."));
    }
    let used = if namespace == SEMANTIC_NAMESPACE {
        semantic_value_used(session, export_id)?
    } else {
        let quoted = format!("\"{id}\"");
        session.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM annotations WHERE class_id=?1) OR EXISTS(SELECT 1 FROM annotation_documents WHERE instr(state_json, ?2) > 0)",
            params![id, quoted],
            |r| r.get(0),
        )?
    };
    if used {
        return Err(invalid(
            "This class is used by saved labels. Reassign or erase those labels first; renaming is always safe.",
        ));
    }
    let deleted = session
        .connection
        .execute(
            "DELETE FROM classes WHERE id=?1 AND project_id=?2 AND namespace=?3",
            params![id, project_id, namespace],
        )
        .map_err(|_| invalid("This class is still referenced and cannot be deleted."))?;
    if deleted != 1 {
        return Err(invalid("Unknown class."));
    }
    current(session)
}
