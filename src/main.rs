use clap::{App, Arg};
use sendgrid::template;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod manage;

fn list() {}

pub fn read_all_templates<P: AsRef<Path>>(dir: P) -> HashMap<String, manage::Template> {
    let p: &Path = dir.as_ref();
    assert!(
        !p.is_dir(),
        "load from directory failed: file not supported"
    );

    let mut templates = HashMap::new();
    if let Ok(entries) = p.read_dir() {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let path = entry.path();
                        if path.ends_with(".mailtemplate") {
                            if let Ok(Ok(template)) = manage::read_template(path) {
                                templates.insert(template.name.clone(), template);
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
        );

    let matches = app.get_matches();

    if let Some(m) = matches.subcommand_matches(SUBCMD_SYNC_TO_DIR) {
        let api_key = matches.value_of(ARG_API_KEY);
        let dir = m.value_of(ARG_DIR).unwrap();
        sync_to_directory(api_key, dir.as_ref()).await
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
            name: t.name,
            plain_body: long.plain_content.unwrap(),
            html_body: long.html_content.unwrap(),
        };

        //println!("{:?}", long);
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
        //println!("templ: {}")
        } else {
            manage::write_template(filename, &template).unwrap();
        }
    }
    Ok(())
}
