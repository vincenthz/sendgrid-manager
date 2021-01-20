use clap::{App, Arg};
use sendgrid::template;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod manage;

pub fn read_all_templates<P: AsRef<Path>>(dir: P) -> HashMap<String, manage::Template> {
    let p: &Path = dir.as_ref();

    let mut templates = HashMap::new();
    if let Ok(entries) = p.read_dir() {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let path = entry.path();
                        let ext = path.extension();
                        if ext.map(|e| e.to_str()) == Some(Some("mailtemplate")) {
                            let r = manage::read_template(path);
                            if let Ok(Ok(template)) = r {
                                templates.insert(template.name.clone(), template);
                            } else {
                                println!("{:?} {:?}", entry, r.err())
                            }
                        }
                    }
                }
            }
        }
    }
    templates
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    const ARG_API_KEY: &str = "API-KEY";
    const ARG_DIR: &str = "DIR";

    const SUBCMD_SYNC_TO_DIR: &str = "sync-to-dir";
    const SUBCMD_CHECK: &str = "check";

    let app = App::new("sendgrid-manager")
        .version("0.1")
        .author("HexDev")
        .about("sendgrid manager")
        .arg(
            Arg::new(ARG_API_KEY)
                .short('k')
                .long("key")
                .value_name("API-KEY")
                .about("Set the API key to use to communicate to sendgrid")
                .takes_value(true),
        )
        .subcommand(
            App::new(SUBCMD_SYNC_TO_DIR).arg(
                Arg::new(ARG_DIR)
                    .takes_value(true)
                    .required(true)
                    .value_name("DIR")
                    .about("directory where the template will be stored"),
            ),
        )
        .subcommand(
            App::new(SUBCMD_CHECK).arg(
                Arg::new(ARG_DIR)
                    .takes_value(true)
                    .required(true)
                    .value_name("DIR")
                    .about("directory where the template are stored"),
            ),
        );
    let matches = app.get_matches();

    if let Some(m) = matches.subcommand_matches(SUBCMD_SYNC_TO_DIR) {
        let api_key = matches.value_of(ARG_API_KEY);
        let dir = m.value_of(ARG_DIR).unwrap();
        sync_to_directory(api_key, dir.as_ref()).await
    } else if let Some(m) = matches.subcommand_matches(SUBCMD_CHECK) {
        let api_key = matches.value_of(ARG_API_KEY);
        let dir = m.value_of(ARG_DIR).unwrap();
        check_against_local(api_key, dir.as_ref()).await
    } else if let Some(name) = matches.subcommand_name() {
        panic!("unknown command {}", name);
    } else {
        panic!("no command");
    }
}

fn get_api_key(arg_api_key: Option<&str>) -> String {
    match arg_api_key {
        None => {
            let sg_api_key = std::env::var("SG_API_KEY")
                .expect("SG_API_KEY environment to be set or by command line argument");
            sg_api_key
        }
        Some(a) => a.to_string(),
    }
}

async fn list_version_remote_active(
    sg_api_key: &str,
    templates: &[template::Template],
) -> Vec<template::TemplateVersion> {
    let mut found = Vec::new();
    for t in templates {
        if t.versions.len() == 0 {
            continue;
        }

        let active_versions = t
            .versions
            .iter()
            .filter(|v| v.active == 1)
            .collect::<Vec<_>>();
        if active_versions.len() != 1 {
            continue;
        }

        let active = active_versions[0];

        let long: template::TemplateVersion = template::get_version(&sg_api_key, &t.id, &active.id)
            .await
            .expect("sendgrid get version");
        found.push(long)
    }
    found
}

async fn check_against_local(
    api_key: Option<&str>,
    dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let sg_api_key = get_api_key(api_key);

    let local_templates = read_all_templates(dir);

    println!(
        "{} local templates found in {}",
        local_templates.len(),
        dir.to_str().expect("valid utf8 dir")
    );

    if local_templates.len() == 0 {
        return Ok(());
    }

    let remote_templates = template::list(&sg_api_key)
        .await
        .expect("sendgrid list result");

    println!("{} remote templates found", remote_templates.len());

    let remote_templates = list_version_remote_active(&sg_api_key, &remote_templates).await;

    for ltmp in local_templates.values() {
        match remote_templates.iter().find(|l| l.name == ltmp.name) {
            None => {
                println!("cannot find remote for \"{}\"", ltmp.name)
            }
            Some(found) => {
                let content_match = &ltmp.html_body == found.html_content.as_deref().unwrap_or("")
                    && &ltmp.plain_body == found.plain_content.as_deref().unwrap_or("");
                println!(
                    "found local \"{}\" as remote={} with content matching {}",
                    ltmp.name, found.template_id, content_match
                )
            }
        }
    }

    Ok(())
}

async fn sync_to_directory(
    api_key: Option<&str>,
    dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let sg_api_key = get_api_key(api_key);

    let templates = template::list(&sg_api_key)
        .await
        .expect("sendgrid list result");

    for t in templates {
        println!("####################################################################");
        println!("ID         : {}", t.id);
        println!("NAME       : {}", t.name);
        println!("Generation : {:?}", t.generation);
        println!("Updated    : {:?}", t.updated_at);
        println!("Versions   : {}", t.versions.len());
        if t.versions.len() == 0 {
            continue;
        }

        let active_versions = t
            .versions
            .iter()
            .filter(|v| v.active == 1)
            .collect::<Vec<_>>();
        if active_versions.len() == 0 {
            println!("**** no active version ****");
            continue;
        }

        if active_versions.len() > 1 {
            println!(
                "**** multiple active versions {} ****",
                active_versions.len()
            );
            continue;
        }

        let active = active_versions[0];

        println!("   * id={}", active.id);
        println!("     name={}", active.name);
        println!("     temp={}", active.template_id);
        println!("     active={}", active.active);

        let long = template::get_version(&sg_api_key, &t.id, &active.id)
            .await
            .expect("sendgrid get version");

        let mut filename = PathBuf::from(dir);
        filename.push(format!("{}.mailtemplate", t.id));

        let template = manage::Template {
            name: long.name,
            plain_body: long.plain_content.unwrap(),
            html_body: long.html_content.unwrap(),
        };

        if let Ok(r) = manage::read_template(&filename) {
            if let Ok(r_templ) = r {
                if &r_templ == &template {
                    println!("same template");
                    continue;
                }
                if r_templ.plain_body != template.plain_body {
                    println!("plain body vary");
                }
                if r_templ.html_body != template.html_body {
                    println!("html body vary");
                }
                let mut tmp_filename = filename.clone();
                tmp_filename.set_extension(".mailtemplate.tmp");
                println!("WRITING tmp file {:?}", tmp_filename.as_path().to_str());
                manage::write_template(&tmp_filename, &template).unwrap();
            } else {
                println!("error while reading template {:?}", r)
            }
        } else {
            manage::write_template(filename, &template).unwrap();
        }
    }
    Ok(())
}
