use base64::{engine::general_purpose::STANDARD, Engine as _};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) struct BuiltinDesign {
    pub key: &'static str,
    pub category: &'static str,
    pub name: &'static str,
    pub image: &'static [u8],
}

pub(crate) const BUILTIN_DESIGNS: &[BuiltinDesign] = &[
    BuiltinDesign {
        key: "sewing-2026-10-01",
        category: "الجيب",
        name: "جيب رسمي",
        image: include_bytes!("../assets/designs/الجيب/جيب رسمي.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-02",
        category: "الجيب",
        name: "جيب غطاء",
        image: include_bytes!("../assets/designs/الجيب/جيب غطاء.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-03",
        category: "الجيب",
        name: "جيب مدور",
        image: include_bytes!("../assets/designs/الجيب/جيب مدور.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-04",
        category: "الجيب",
        name: "جيب مقوس",
        image: include_bytes!("../assets/designs/الجيب/جيب مقوس.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-05",
        category: "الجيب",
        name: "مربع",
        image: include_bytes!("../assets/designs/الجيب/مربع.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-06",
        category: "اليد السادة",
        name: "ساده",
        image: include_bytes!("../assets/designs/اليد/ساده.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-07",
        category: "الكبك",
        name: "كبك قماش جبزور",
        image: include_bytes!("../assets/designs/اليد/كبك قماش جبزور.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-08",
        category: "الكبك",
        name: "كبك قماش",
        image: include_bytes!("../assets/designs/اليد/كبك قماش.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-09",
        category: "الكبك",
        name: "مقوى فرنسي",
        image: include_bytes!("../assets/designs/اليد/مقوى فرنسي.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-10",
        category: "الكبك",
        name: "مقوى مدور",
        image: include_bytes!("../assets/designs/اليد/مقوى مدور.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-11",
        category: "الكبك",
        name: "مقوى مربع",
        image: include_bytes!("../assets/designs/اليد/مقوى مربع.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-12",
        category: "اليد السادة",
        name: "نص كم",
        image: include_bytes!("../assets/designs/اليد/نص كم.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-13",
        category: "جبزور",
        name: "مثلث زرار مخفي",
        image: include_bytes!("../assets/designs/جبزور/مثلث  زرار مخفي.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-14",
        category: "جبزور",
        name: "مثلث زرار",
        image: include_bytes!("../assets/designs/جبزور/مثلث زرار.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-15",
        category: "جبزور",
        name: "مثلث سحاب",
        image: include_bytes!("../assets/designs/جبزور/مثلث سحاب.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-16",
        category: "جبزور",
        name: "مربع زرار مخفي",
        image: include_bytes!("../assets/designs/جبزور/مربع   زرار مخفي.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-17",
        category: "جبزور",
        name: "مربع زرار",
        image: include_bytes!("../assets/designs/جبزور/مربع زرار.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-18",
        category: "جبزور",
        name: "مربع سحاب",
        image: include_bytes!("../assets/designs/جبزور/مربع سحاب.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-19",
        category: "القلاب",
        name: "فرنسي أصغر",
        image: include_bytes!("../assets/designs/قلاب/فرنسي   أصغر.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-20",
        category: "القلاب",
        name: "فرنسي صغير",
        image: include_bytes!("../assets/designs/قلاب/فرنسي صغير.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-21",
        category: "القلاب",
        name: "فرنسي كبير 2",
        image: include_bytes!("../assets/designs/قلاب/فرنسي كبير 2.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-22",
        category: "القلاب",
        name: "فرنسي كبير",
        image: include_bytes!("../assets/designs/قلاب/فرنسي كبير.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-23",
        category: "القلاب",
        name: "قلاب مفروش",
        image: include_bytes!("../assets/designs/قلاب/قلاب مفروش.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-24",
        category: "القلاب",
        name: "قلاب ملكي",
        image: include_bytes!("../assets/designs/قلاب/قلاب ملكي.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-25",
        category: "القلاب",
        name: "مدور",
        image: include_bytes!("../assets/designs/قلاب/مدور.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-26",
        category: "القلاب",
        name: "ملكي كبير 2",
        image: include_bytes!("../assets/designs/قلاب/ملكي كبير 2.jpeg"),
    },
    BuiltinDesign {
        key: "sewing-2026-10-27",
        category: "القلاب",
        name: "ملكي كبير",
        image: include_bytes!("../assets/designs/قلاب/ملكي كبير.jpeg"),
    },
];

pub(crate) fn seed_builtin_designs(conn: &Connection) -> Result<(), String> {
    let present: i64 = conn.query_row(
        "SELECT COUNT(*) FROM design_options WHERE builtin_key IS NOT NULL",
        [],
        |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    if present >= BUILTIN_DESIGNS.len() as i64 { return Ok(()); }
    for item in BUILTIN_DESIGNS {
        let exists: Option<i64> = conn.query_row(
            "SELECT id FROM design_options WHERE builtin_key=?1",
            [item.key],
            |row| row.get(0),
        ).optional().map_err(|error| error.to_string())?;
        if exists.is_some() { continue; }
        let image_data = format!("data:image/jpeg;base64,{}", STANDARD.encode(item.image));
        conn.execute(
            "INSERT OR IGNORE INTO design_options(category,name,image_data,created_at,builtin_key) VALUES(?1,?2,?3,datetime('now','localtime'),?4)",
            params![item.category,item.name,image_data,item.key],
        ).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeds_are_idempotent_and_have_valid_images() {
        let conn=Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE design_options(id INTEGER PRIMARY KEY,category TEXT,name TEXT,image_data TEXT,created_at TEXT)").unwrap();
        conn.execute("INSERT INTO design_options(category,name,image_data,created_at) VALUES('الجيب','شكل خاص','data:image/jpeg;base64,custom','2026-09-01')",[]).unwrap();
        conn.execute_batch("ALTER TABLE design_options ADD COLUMN builtin_key TEXT; CREATE UNIQUE INDEX idx_design_options_builtin_key ON design_options(builtin_key)").unwrap();
        seed_builtin_designs(&conn).unwrap();
        let count: i64=conn.query_row("SELECT COUNT(*) FROM design_options",[],|row|row.get(0)).unwrap();
        assert_eq!(count,28);
        seed_builtin_designs(&conn).unwrap();
        let second: i64=conn.query_row("SELECT COUNT(*) FROM design_options",[],|row|row.get(0)).unwrap();
        assert_eq!(second,count);
        let custom: (String,Option<String>)=conn.query_row("SELECT name,builtin_key FROM design_options WHERE id=1",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(custom,("شكل خاص".into(),None));
        for item in BUILTIN_DESIGNS { assert!(item.image.starts_with(&[0xff,0xd8,0xff])); }
    }
}
