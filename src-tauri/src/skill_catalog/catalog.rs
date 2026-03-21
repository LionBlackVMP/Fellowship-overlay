use serde::{Deserialize, Serialize};
use std::fs::{self, read_dir};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
pub struct Ability {
    pub id: u32,
    pub name: String,
    pub cooldown: u32,
    pub icon: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Class {
    pub id: u32,
    pub name: String,
    pub abilities: Vec<Ability>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SkillCatalog {
    pub classes: Vec<Class>,
}

fn normalize_name(raw: &str) -> String {
    let s = raw
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '_' || c == '-')
        .replace(&['_', '-'][..], " ");
    s.trim().to_string()
}

pub fn build_catalog(skills_path: &Path, heroes_dir: &Path) -> SkillCatalog {
    let data = fs::read_to_string(skills_path).unwrap_or_else(|_| "{}".to_string());
    let skills_json: serde_json::Value = serde_json::from_str(&data).unwrap_or(serde_json::json!({}));

    let mut hero_folders = std::collections::HashMap::new();
    if let Ok(entries) = read_dir(heroes_dir) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if !ft.is_dir() { continue; }
            }
            let name = entry.file_name().into_string().unwrap_or_default();
            if let Some(caps) = name.split_once('_').or_else(|| name.split_once('-')) {
                hero_folders.insert(caps.0.to_string(), (name.clone(), normalize_name(caps.1)));
            }
        }
    }

    let mut classes = Vec::new();

    for (class_id_str, abilities) in skills_json.as_object().unwrap_or(&serde_json::Map::new()) {
        let class_id = class_id_str.parse::<u32>().unwrap_or(0);
        let (dir_name, class_name) = hero_folders.get(class_id_str)
            .map(|(d, n)| (d.clone(), n.clone()))
            .unwrap_or((format!("Class {}", class_id), format!("Class {}", class_id)));

        let mut ability_list = Vec::new();
        if let Some(abilities_obj) = abilities.as_object() {
            for (ability_id_str, cooldown) in abilities_obj {
                let ability_id = ability_id_str.parse::<u32>().unwrap_or(0);
                let cd = cooldown.as_u64().unwrap_or(0) as u32;

                let mut ability_name = format!("Skill {}", ability_id);
                let mut icon_path = None;

                let hero_path = heroes_dir.join(&dir_name);
                if let Ok(files) = read_dir(&hero_path) {
                    for file in files.flatten() {
                        let file_name = file.file_name().into_string().unwrap_or_default();
                        if file_name.starts_with(&ability_id.to_string()) {
                            ability_name = normalize_name(&file_name);
                            icon_path = Some(format!("Heroes/{}/{}", dir_name, file_name));
                            break;
                        }
                    }
                }

                ability_list.push(Ability {
                    id: ability_id,
                    name: ability_name,
                    cooldown: cd,
                    icon: icon_path,
                });
            }
        }

        ability_list.sort_by(|a, b| a.name.cmp(&b.name));

        classes.push(Class {
            id: class_id,
            name: class_name,
            abilities: ability_list,
        });
    }

    classes.sort_by(|a, b| a.name.cmp(&b.name));

    SkillCatalog { classes }
}